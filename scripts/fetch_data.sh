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
#   * qwerty-learner    (RealKai42, GPL-3.0)       -> data/dicts/PEPXiaoXue*.json, GaoKao_3500.json
#   * TEM4/TEM8 lists   (kajweb/dict, word list only; definitions come from MIT ECDICT)
#
# After fetching, run:  python3 scripts/build_dict_db.py
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="$ROOT/data"
DICTS="$DATA/dicts"
mkdir -p "$DICTS"

RAW_ECDICT="https://raw.githubusercontent.com/skywind3000/ECDICT/master"
RAW_QWERTY="https://raw.githubusercontent.com/RealKai42/qwerty-learner/master/public/dicts"
RAW_KAJWEB="https://raw.githubusercontent.com/kajweb/dict/master/book"

fetch() {  # fetch <url> <out>
  local url="$1" out="$2"
  if [[ -s "$out" ]]; then
    echo "  = exists: $(basename "$out")"
    return 0
  fi
  echo "  -> $(basename "$out")"
  curl -sS -L --retry 3 --retry-delay 2 -o "$out" "$url"
}

echo "[1/4] ECDICT master dictionary (MIT)"
fetch "$RAW_ECDICT/ecdict.csv"   "$DATA/ecdict.csv"
fetch "$RAW_ECDICT/wordroot.txt" "$DATA/wordroot.txt"

echo "[2/4] qwerty-learner graded lists (小学 PEP + 高考3500)"
for f in PEPXiaoXue3_1_T PEPXiaoXue3_2_T PEPXiaoXue4_1_T PEPXiaoXue4_2_T \
         PEPXiaoXue5_1_T PEPXiaoXue5_2_T PEPXiaoXue6_1_T PEPXiaoXue6_2_T \
         GaoKao_3500; do
  fetch "$RAW_QWERTY/$f.json" "$DICTS/$f.json"
done

echo "[3/4] TEM4 / TEM8 syllabus word lists (words only)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fetch "$RAW_KAJWEB/1521164653685_Level4_2.zip" "$TMP/level4.zip"
fetch "$RAW_KAJWEB/1521164663794_Level8_2.zip" "$TMP/level8.zip"
unzip -o -q "$TMP/level4.zip" -d "$TMP/l4"
unzip -o -q "$TMP/level8.zip" -d "$TMP/l8"

echo "[4/4] Extracting headwords -> data/dicts/tem{4,8}.json"
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
