#!/usr/bin/env bash
# Fetch the source datasets used to build assets/synstudy.db.
#
# All downloads honour the standard http_proxy / https_proxy environment
# variables (curl reads them automatically), e.g.:
#
#   export http_proxy=http://127.0.0.1:7890
#   export https_proxy=http://127.0.0.1:7890
#   bash scripts/fetch_data.sh
#
# Sources & licences:
#   * ECDICT            (skywind3000/ECDICT, MIT)  -> data/ecdict.csv, data/wordroot.txt
#   * CC-CEDICT         (MDBG, CC-BY-SA 4.0)        -> data/cedict.txt   (真·汉英词典)
#   * CEFR-J            (openlanguageprofiles/olp-en-cefrj, 需引用署名) -> data/cefrj/*.csv
#   * Wiktionary/kaikki.org ( kaikki.org-dictionary-English.jsonl, CC BY-SA 4.0) -> data/kaikki.en.jsonl  (例句)
#   * Tatoeba           (tatoeba.org, CC-BY 2.0 FR)  -> data/tatoeba/*.tsv.bz2  (分级句库)
#   * OpenCC            (BYVoid/OpenCC, Apache-2.0)  -> data/opencc/TS*.txt  (繁→简转换表)
#   * qwerty-learner    (RealKai42, GPL-3.0)       -> data/dicts/PEPXiaoXue*.json, GaoKao_3500.json
#   * TEM4/TEM8 lists   (kajweb/dict, word list only; definitions come from MIT ECDICT)
#
# Set SKIP_TATOEBA_LINKS=1 to skip the large (~200MB) cross-language links
# archive; the sentence bank still builds, just without Chinese translations.
#
# After fetching, run:  python3 scripts/build_dict_db.py
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="$ROOT/data"
DICTS="$DATA/dicts"
CEFRJ="$DATA/cefrj"
TATOEBA="$DATA/tatoeba"
OPENCC="$DATA/opencc"
mkdir -p "$DICTS" "$CEFRJ" "$TATOEBA" "$OPENCC"

RAW_ECDICT="https://raw.githubusercontent.com/skywind3000/ECDICT/master"
RAW_QWERTY="https://raw.githubusercontent.com/RealKai42/qwerty-learner/master/public/dicts"
RAW_KAJWEB="https://raw.githubusercontent.com/kajweb/dict/master/book"
RAW_CEFRJ="https://raw.githubusercontent.com/openlanguageprofiles/olp-en-cefrj/master"
CEDICT_GZ="https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz"
KAIKKI="https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl"
# Tatoeba renamed per-language exports to ISO639-3 codes (en -> eng); the old
# en_sentences URL returns 404, so we use eng and keep the local file name stable.
TATOEBA_EN="https://downloads.tatoeba.org/exports/per_language/eng/eng_sentences.tsv.bz2"
TATOEBA_CMN="https://downloads.tatoeba.org/exports/per_language/cmn/cmn_sentences.tsv.bz2"
TATOEBA_LINKS="https://downloads.tatoeba.org/exports/links.tar.bz2"
# OpenCC t2s dictionaries (the S->T named files are the opposite direction;
# t2s uses TS*). jsdelivr mirrors the GitHub repo and is reachable where
# raw.githubusercontent.com is not.
OPENCC_TS="https://cdn.jsdelivr.net/gh/BYVoid/OpenCC@master/data/dictionary"

fetch() {  # fetch <url> <out>
  local url="$1" out="$2"
  if [[ -s "$out" ]]; then
    echo "  = exists: $(basename "$out")"
    return 0
  fi
  echo "  -> $(basename "$out")"
  # --fail: on 404/5xx curl exits non-zero WITHOUT writing the error page to
  # the output file (a saved HTML body would poison the later bz2 decode).
  curl -sS -L -f --retry 3 --retry-delay 2 -o "$out" "$url"
}

echo "[1/8] ECDICT master dictionary (MIT)"
fetch "$RAW_ECDICT/ecdict.csv"   "$DATA/ecdict.csv"
fetch "$RAW_ECDICT/wordroot.txt" "$DATA/wordroot.txt"

echo "[2/8] CC-CEDICT Chinese-English dictionary (CC-BY-SA 4.0)"
if [[ -s "$DATA/cedict.txt" ]]; then
  echo "  = exists: cedict.txt"
else
  echo "  -> cedict.txt"
  curl -sS -L --retry 3 --retry-delay 2 -o "$DATA/cedict.txt.gz" "$CEDICT_GZ"
  gunzip -f "$DATA/cedict.txt.gz"
fi

echo "[3/8] CEFR-J vocabulary profile (A1-C2 levels; requires citation)"
fetch "$RAW_CEFRJ/cefrj-vocabulary-profile-1.5.csv"         "$CEFRJ/cefrj-vocabulary-profile-1.5.csv"
fetch "$RAW_CEFRJ/octanove-vocabulary-profile-c1c2-1.0.csv" "$CEFRJ/octanove-vocabulary-profile-c1c2-1.0.csv"
fetch "$RAW_CEFRJ/README.md"                               "$CEFRJ/olp-en-cefrj-README.md"

echo "[4/8] Wiktionary example sentences (kaikki.org, CC BY-SA 4.0; ~3.1GB)"
if [[ -s "$DATA/kaikki.en.jsonl" ]]; then
  echo "  = exists: kaikki.en.jsonl"
else
  echo "  -> kaikki.en.jsonl (large; supports resume via -C -)"
  curl -sS -L -f --retry 3 --retry-delay 2 -C - -o "$DATA/kaikki.en.jsonl" "$KAIKKI"
fi

echo "[5/8] Tatoeba sentence bank (CC-BY 2.0 FR; graded example sentences)"
fetch "$TATOEBA_EN"  "$TATOEBA/en_sentences.tsv.bz2"
fetch "$TATOEBA_CMN" "$TATOEBA/cmn_sentences.tsv.bz2"
# Tatoeba/cmn translations are mostly Traditional; the builder converts them
# to Simplified with these OpenCC tables (Apache-2.0).
fetch "$OPENCC_TS/TSCharacters.txt" "$OPENCC/TSCharacters.txt"
fetch "$OPENCC_TS/TSPhrases.txt"    "$OPENCC/TSPhrases.txt"
if [[ "${SKIP_TATOEBA_LINKS:-0}" == "1" ]]; then
  echo "  ! SKIP_TATOEBA_LINKS=1 set; 句库将不包含中文翻译"
else
  if [[ -s "$TATOEBA/links.tsv.bz2" || -s "$TATOEBA/links.tsv" ]]; then
    echo "  = exists: links"
  else
    echo "  -> links.tar.bz2 (~200MB; contains links.csv)"
    curl -sS -L -f --retry 3 --retry-delay 2 -C - -o "$TATOEBA/links.tar.bz2" "$TATOEBA_LINKS"
    # Extract links.csv -> links.tsv (Tatoeba's links.csv is already tab-separated
    # despite the extension; rename for consistency with the builder's glob).
    tar -xjf "$TATOEBA/links.tar.bz2" -C "$TATOEBA" links.csv 2>/dev/null || \
      tar -xjf "$TATOEBA/links.tar.bz2" -C "$TATOEBA"
    if [[ -f "$TATOEBA/links.csv" ]]; then
      mv "$TATOEBA/links.csv" "$TATOEBA/links.tsv"
      rm -f "$TATOEBA/links.tar.bz2"
    fi
  fi
fi

echo "[6/8] qwerty-learner graded lists (小学 PEP + 高考3500)"
for f in PEPXiaoXue3_1_T PEPXiaoXue3_2_T PEPXiaoXue4_1_T PEPXiaoXue4_2_T \
         PEPXiaoXue5_1_T PEPXiaoXue5_2_T PEPXiaoXue6_1_T PEPXiaoXue6_2_T \
         GaoKao_3500; do
  fetch "$RAW_QWERTY/$f.json" "$DICTS/$f.json"
done

echo "[7/8] TEM4 / TEM8 syllabus word lists (words only)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fetch "$RAW_KAJWEB/1521164653685_Level4_2.zip" "$TMP/level4.zip"
fetch "$RAW_KAJWEB/1521164663794_Level8_2.zip" "$TMP/level8.zip"
unzip -o -q "$TMP/level4.zip" -d "$TMP/l4"
unzip -o -q "$TMP/level8.zip" -d "$TMP/l8"

echo "[8/8] Extracting headwords -> data/dicts/tem{4,8}.json"
python3 - "$TMP" "$DICTS" <<'PY'
import json, os, sys
tmp, dicts = sys.argv[1], sys.argv[2]
def extract(src, out):
    words, seen = [], set()
    with open(src, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                o = json.loads(line)
            except Exception:
                continue
            w = (o.get("headWord") or "").strip()
            if not w or w in seen:
                continue
            seen.add(w)
            words.append({"name": w})
    with open(out, "w", encoding="utf-8") as f:
        json.dump(words, f, ensure_ascii=False)
    print("  -> {}: {} words".format(os.path.basename(out), len(words)))
extract(os.path.join(tmp, "l4", "Level4_2.json"), os.path.join(dicts, "tem4.json"))
extract(os.path.join(tmp, "l8", "Level8_2.json"), os.path.join(dicts, "tem8.json"))
PY

echo
echo "Done. Now build the database:"
echo "  python3 scripts/build_dict_db.py"
