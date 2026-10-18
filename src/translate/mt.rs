//! HY-MT1.5-1.8B machine translation via `llama-cpp-2` (llama.cpp FFI).
//!
//! The expensive part is loading the GGUF weights, so [`Translator`] keeps the
//! backend + model resident and creates a fresh `LlamaContext` per translation
//! (context creation only allocates the KV cache). Decoding is greedy — the
//! deterministic choice for translation — implemented directly over the logits
//! (`LlamaContext::get_logits_ith`) to keep the FFI surface minimal.
//!
//! GPU offload is controlled by `n_gpu_layers`: a large value pushes every layer
//! onto Metal (Apple, enabled automatically by llama.cpp) / Vulkan / CUDA when
//! the crate was built with that backend; `0` forces CPU.

use std::collections::HashSet;
use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::llama_backend::LlamaBackend;

use super::models::{self, ModelStatus, MT_MODEL};

/// Build the HY-MT (Hunyuan-MT) instruction prompt.
///
/// Hunyuan-MT was trained on two distinct templates and expects the right one:
/// - **Chinese involved** (source or target is Chinese):
///   `把下面的文本翻译成{目标语言}，不要额外解释。\n\n{text}`
/// - **any other pair**:
///   `Translate the following segment from {src} into {target}, without additional explanation.\n\n{text}`
///   (the `from {src}` clause is dropped when the source is unknown/auto).
///
/// Using the wrong template pushes the prompt out of distribution and the model
/// degenerates (garbage / endless repetition), so this mirrors the official
/// formats. `src` of "" / "auto" means "let the model detect the source".
pub fn build_prompt(text: &str, target: &str, src: &str) -> String {
    let target = if target.trim().is_empty() {
        "Chinese"
    } else {
        target.trim()
    };
    let src = src.trim();
    let src_auto = src.is_empty() || src.eq_ignore_ascii_case("auto");

    let involves_zh = target.eq_ignore_ascii_case("Chinese")
        || (!src_auto && src.eq_ignore_ascii_case("Chinese"));

    if involves_zh {
        format!(
            "把下面的文本翻译成{}，不要额外解释。\n\n{text}",
            zh_name(target)
        )
    } else if src_auto {
        format!("Translate the following segment into {target}, without additional explanation.\n\n{text}")
    } else {
        format!("Translate the following segment from {src} into {target}, without additional explanation.\n\n{text}")
    }
}

/// The Chinese display name for a language value (e.g. "English" -> "英语"),
/// falling back to the value itself when it is not one of the known
/// [`super::LANGS`]. The zh template names the target language in Chinese.
fn zh_name(lang: &str) -> &str {
    super::LANGS
        .iter()
        .find(|(v, _)| v.eq_ignore_ascii_case(lang))
        .map(|(_, label)| *label)
        .unwrap_or(lang)
}

/// A resident translation engine (backend + weights). Create with [`Translator::load`].
pub struct Translator {
    // Field order matters for drop: the context borrows the model, and the model
    // borrows the backend, so they must be dropped in reverse (ctx is per-call).
    backend: LlamaBackend,
    model: LlamaModel,
    n_ctx: u32,
    /// Token ids that end an assistant turn. The Hunyuan chat template closes a
    /// reply with `<｜hy_place▁holder▁no▁2｜>` (the GGUF's declared eos) and
    /// `<｜hy_place▁holder▁no▁8｜>`; we stop on those plus EOT/EOS so a
    /// translation can never run away into freeform continuation.
    stop_ids: HashSet<i32>,
}

/// The Hunyuan role markers this model's chat template is built from. The GGUF
/// ships no `tokenizer.chat_template`, so we assemble the prompt by hand.
const HY_USER: &str = "<｜hy_User｜>";
const HY_ASSISTANT: &str = "<｜hy_Assistant｜>";
/// End-of-turn control tokens (resolved to ids at load; strings must match the
/// vocab exactly, including the fullwidth `｜` and the `▁` space marker).
const HY_END_TOKENS: [&str; 4] = [
    "<｜hy_place▁holder▁no▁2｜>", // turn separator (== declared eos)
    "<｜hy_place▁holder▁no▁8｜>", // template suffix end marker
    "<｜hy_EOT｜>",               // end of turn
    "<｜hy_end▁of▁sentence｜>",    // end of sentence
];

impl Translator {
    /// Load the GGUF weights. `gpu` enables full-layer offload; `n_ctx` sizes the
    /// context (mobile: 1024, desktop: 2048).
    pub fn load(gpu: bool, n_ctx: u32) -> Result<Self, String> {
        if models::status(&MT_MODEL) != ModelStatus::Ready {
            return Err(format!("翻译模型缺失：{}（请先下载）", MT_MODEL.label));
        }
        let path = models::local_path(&MT_MODEL);
        let backend = LlamaBackend::init().map_err(|e| format!("初始化推理后端失败: {e:?}"))?;
        let model_params =
            LlamaModelParams::default().with_n_gpu_layers(if gpu { u32::MAX } else { 0 });
        let model = LlamaModel::load_from_file(&backend, &path, &model_params)
            .map_err(|e| format!("加载翻译模型失败: {e:?}"))?;
        let stop_ids = Self::resolve_stop_ids(&model);
        Ok(Self {
            backend,
            model,
            n_ctx: n_ctx.max(256),
            stop_ids,
        })
    }

    /// Resolve the end-of-turn token ids by string (never hardcoded), seeded with
    /// whatever the GGUF declares as eos/eot. Unknown strings simply drop out.
    fn resolve_stop_ids(model: &LlamaModel) -> HashSet<i32> {
        let vocab = model.vocab();
        let mut ids = HashSet::new();
        ids.insert(vocab.eos().0);
        ids.insert(vocab.eot().0);
        for s in HY_END_TOKENS {
            let t = vocab.tokenize(s.as_bytes(), false, true);
            if t.len() == 1 {
                ids.insert(t[0].0);
            }
        }
        ids.retain(|&id| id >= 0);
        ids
    }

    /// Translate `text`, streaming each decoded piece to `on_token` as it is
    /// produced. Returns the full translation. Blocking; run off the UI thread.
    pub fn translate(
        &self,
        text: &str,
        target: &str,
        src: &str,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let query = build_prompt(text, target, src);
        // Wrap the instruction in the Hunyuan chat template:
        //   <｜hy_begin▁of▁sentence｜><｜hy_User｜>{query}<｜hy_Assistant｜>
        // (`add_bos` supplies the begin-of-sentence prefix). Without the role
        // framing the model treats the prompt as raw text to *continue* — it
        // writes essays instead of translating and never emits its turn-end
        // token, so generation runs to the cap.
        let prompt = format!("{HY_USER}{query}{HY_ASSISTANT}");
        let ctx_params =
            LlamaContextParams::default().with_n_ctx(NonZeroU32::new(self.n_ctx).or(NonZeroU32::new(512)));
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("创建推理上下文失败: {e:?}"))?;

        let vocab = self.model.vocab();
        let mut tokens = vocab.tokenize(prompt.as_bytes(), true, true);
        if tokens.is_empty() {
            return Err("提示词分词结果为空".to_string());
        }
        // Leave room for generation inside the context window.
        let budget = self.n_ctx as usize;
        if tokens.len() >= budget {
            tokens.truncate(budget.saturating_sub(128).max(1));
        }

        let mut batch = LlamaBatch::new(tokens.len() + 8, 1);
        let last = (tokens.len() - 1) as i32;
        for (i, tok) in tokens.iter().enumerate() {
            batch
                .add(*tok, i as i32, &[0], (i as i32) == last)
                .map_err(|e| format!("构建输入批次失败: {e:?}"))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| format!("预填充 (prefill) 失败: {e:?}"))?;

        let eos = vocab.eos();
        let eot = vocab.eot();
        // Bound generation to a sane multiple of the prompt length: a translation
        // is never many times longer than its source, so this stops a degenerate
        // repetition loop from burning the whole context window.
        let len_cap = tokens.len().saturating_mul(4).saturating_add(160);
        let max_new = budget
            .saturating_sub(tokens.len())
            .min(len_cap)
            .clamp(1, 1024);
        let mut out = String::new();
        let mut pos: i32 = tokens.len() as i32;

        // Frequency of each token id over the prompt + generated tokens so far,
        // used for the repetition penalty below. Sized lazily from the logits.
        let mut counts: Vec<u32> = Vec::new();

        for _ in 0..max_new {
            let idx = batch.n_tokens() - 1;
            let logits = ctx.get_logits_ith(idx);
            if counts.is_empty() {
                counts.resize(logits.len(), 0);
                for &t in &tokens {
                    let i = t.0 as usize;
                    if i < counts.len() {
                        counts[i] += 1;
                    }
                }
            }
            // Greedy argmax, but penalize tokens that already appeared so the
            // model can't lock into an infinite repetition loop.
            let next = argmax_penalized(logits, &counts, REP_PENALTY);
            if next == eos
                || next == eot
                || vocab.is_eog(next)
                || self.stop_ids.contains(&next.0)
            {
                break;
            }
            let ni = next.0 as usize;
            if ni < counts.len() {
                counts[ni] += 1;
            }
            let piece = vocab.token_to_piece(next, false, None);
            if !piece.is_empty() {
                // Token pieces can split a multi-byte char; append lossily so the
                // stream never stalls on a partial sequence.
                let s = String::from_utf8_lossy(&piece);
                out.push_str(&s);
                on_token(&s);
            }

            batch.clear();
            batch
                .add(next, pos, &[0], true)
                .map_err(|e| format!("追加生成 token 失败: {e:?}"))?;
            pos += 1;
            ctx.decode(&mut batch)
                .map_err(|e| format!("解码失败: {e:?}"))?;
        }
        Ok(out.trim().to_string())
    }
}

/// Multiplicative repetition penalty. Now that the turn-end token stops decoding
/// cleanly this is only a safety net, so keep it mild (Hunyuan-MT is sensitive to
/// over-penalizing legitimately repeated terms).
const REP_PENALTY: f32 = 1.05;

/// Index of the largest logit after applying a frequency-scaled repetition
/// penalty to tokens already present in `counts`, returned as a token id.
fn argmax_penalized(logits: &[f32], counts: &[u32], penalty: f32) -> LlamaToken {
    let mut best_idx = 0usize;
    let mut best_val = f32::NEG_INFINITY;
    for (i, &l) in logits.iter().enumerate() {
        let seen = counts.get(i).copied().unwrap_or(0);
        let v = if seen > 0 {
            // Cap the exponent so a token repeated dozens of times can't blow the
            // factor up into inf/NaN territory.
            let factor = penalty.powi(seen.min(6) as i32);
            if l > 0.0 {
                l / factor
            } else {
                l * factor
            }
        } else {
            l
        };
        if v > best_val {
            best_val = v;
            best_idx = i;
        }
    }
    LlamaToken(best_idx as i32)
}

#[cfg(test)]
mod tests {
    use super::build_prompt;

    #[test]
    fn chinese_target_uses_zh_template() {
        let p = build_prompt("Hello world", "Chinese", "auto");
        assert!(p.contains("把下面的文本翻译成中文，不要额外解释。"));
        assert!(p.contains("Hello world"));
        // The zh template carries no English source clause.
        assert!(!p.contains("Translate the following segment"));
    }

    #[test]
    fn zh_to_other_names_target_in_chinese() {
        let p = build_prompt("你好", "English", "Chinese");
        assert!(p.contains("把下面的文本翻译成英语，不要额外解释。"));
        assert!(p.contains("你好"));
    }

    #[test]
    fn non_chinese_pair_with_explicit_source() {
        let p = build_prompt("Bonjour", "English", "French");
        assert!(p.contains("Translate the following segment from French into English"));
        assert!(p.contains("Bonjour"));
    }

    #[test]
    fn non_chinese_pair_auto_source_omits_from_clause() {
        let p = build_prompt("Hola", "German", "auto");
        assert!(p.contains("Translate the following segment into German"));
        assert!(!p.contains(" from "));
    }

    #[test]
    fn empty_target_defaults_to_chinese() {
        let p = build_prompt("Hi", "  ", "auto");
        assert!(p.contains("翻译成中文"));
    }

    /// Loads the real HY-MT GGUF (~1 GB) and checks that a translation is clean
    /// and bounded (no essay / no runaway repetition). Ignored by default.
    #[test]
    #[ignore = "needs the downloaded HY-MT model; run with -- --ignored --nocapture"]
    fn live_translation_is_clean() {
        use super::Translator;
        let t = Translator::load(true, 2048).expect("HY-MT model must be downloaded");
        for (text, target) in [
            ("It's on the house.", "Chinese"),
            ("Hope", "Chinese"),
            ("你好，世界", "English"),
        ] {
            let mut sink = |_: &str| {};
            let out = t.translate(text, target, "auto", &mut sink).unwrap();
            eprintln!("[{text} -> {target}] {out}");
            assert!(!out.trim().is_empty(), "empty translation for {text:?}");
            // A single short sentence should never expand into an essay.
            assert!(
                out.chars().count() < 200,
                "translation ran away for {text:?}: {} chars",
                out.chars().count()
            );
        }
    }
}
