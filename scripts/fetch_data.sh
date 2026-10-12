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
#   * WordNet 3.1       (Princeton, BSD-style licence) -> data/wordnet/wordnet31/  (近义/反义补充)
#
# NOTE: 小学/专四/专八 word lists are NOT fetched here. The previous sources
# (qwerty-learner, GPL-3.0; kajweb/dict, no licence) were removed for licence
# compliance. Drop your own official-syllabus lists at data/dicts/{xiaoxue,
# tem4,tem8}.json before building; those categories are skipped when absent.
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
RAW_CEFRJ="https://raw.githubusercontent.com/openlanguageprofiles/olp-en-cefrj/master"
CEDICT_GZ="https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz"
KAIKKI="https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl"
# Tatoeba renamed per-language exports to ISO639-3 codes (en -> eng); the old
# en_sentences URL returns 404, so we use eng and keep the local file name stable.
TATOEBA_EN="https://downloads.tatoeba.org/exports/per_language/eng/eng_sentences.tsv.bz2"
TATOEBA_CMN="https://downloads.tatoeba.org/exports/per_language/cmn/cmn_sentences.tsv.bz2"
TATOEBA_LINKS="https://downloads.tatoeba.org/exports/links.tar.bz2"
# WordNet 3.1 dict files. Princeton's own wn3.1.dict.tar.gz URLs are dead;
# NLTK's wordnet31 corpus package ships the identical data.*/index.* files and
# is served through jsdelivr (reachable where raw.githubusercontent is slow).
WORDNET_ZIP="https://cdn.jsdelivr.net/gh/nltk/nltk_data@gh-pages/packages/corpora/wordnet31.zip"
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

echo "[1/6] ECDICT master dictionary (MIT)"
fetch "$RAW_ECDICT/ecdict.csv"   "$DATA/ecdict.csv"
fetch "$RAW_ECDICT/wordroot.txt" "$DATA/wordroot.txt"

echo "[2/6] CC-CEDICT Chinese-English dictionary (CC-BY-SA 4.0)"
if [[ -s "$DATA/cedict.txt" ]]; then
  echo "  = exists: cedict.txt"
else
  echo "  -> cedict.txt"
  curl -sS -L --retry 3 --retry-delay 2 -o "$DATA/cedict.txt.gz" "$CEDICT_GZ"
  gunzip -f "$DATA/cedict.txt.gz"
fi

echo "[3/6] CEFR-J vocabulary profile (A1-C2 levels; requires citation)"
fetch "$RAW_CEFRJ/cefrj-vocabulary-profile-1.5.csv"         "$CEFRJ/cefrj-vocabulary-profile-1.5.csv"
fetch "$RAW_CEFRJ/octanove-vocabulary-profile-c1c2-1.0.csv" "$CEFRJ/octanove-vocabulary-profile-c1c2-1.0.csv"
fetch "$RAW_CEFRJ/README.md"                               "$CEFRJ/olp-en-cefrj-README.md"

echo "[4/6] Wiktionary example sentences (kaikki.org, CC BY-SA 4.0; ~3.1GB)"
if [[ -s "$DATA/kaikki.en.jsonl" ]]; then
  echo "  = exists: kaikki.en.jsonl"
else
  echo "  -> kaikki.en.jsonl (large; supports resume via -C -)"
  curl -sS -L -f --retry 3 --retry-delay 2 -C - -o "$DATA/kaikki.en.jsonl" "$KAIKKI"
fi

echo "[5/6] Tatoeba sentence bank (CC-BY 2.0 FR; graded example sentences)"
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

echo "[6/6] WordNet 3.1 dict files (Princeton licence; synonym/antonym supplement)"
WN="$ROOT/data/wordnet"
if [[ -s "$WN/wordnet31/data.adj" && -s "$WN/wordnet31/data.noun" ]]; then
  echo "  = exists: wordnet31/"
else
  echo "  -> wordnet31.zip (~11MB)"
  mkdir -p "$WN"
  fetch "$WORDNET_ZIP" "$WN/wordnet31.zip"
  unzip -q -t "$WN/wordnet31.zip"   # abort before poisoning the dir on a bad zip
  unzip -qo "$WN/wordnet31.zip" -d "$WN"
fi

echo
echo "Done. Now build the database:"
echo "  python3 scripts/build_dict_db.py"
