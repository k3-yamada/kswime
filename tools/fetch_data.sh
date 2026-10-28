#!/usr/bin/env bash
# Downloads the public data the model is built from.  Small files (romaji
# table, POS ids, bunsetsu rules, licence texts, en_extra.txt, en_scowl70.txt)
# are committed; this script fetches the large ones and regenerates the
# derived tables.  See NOTICE for licences.
#   Mozc dictionary / connection matrix          BSD-3-Clause + IPAdic notices
#   Google Books Ngram v3 via hackerb9/gwordlist  CC BY 3.0
#   SCOWL / ESDB (en-wl/wordlist)                 MIT-like (Kevin Atkinson)
#   CMU Pronouncing Dictionary                    BSD-2-Clause (CMU)
set -euo pipefail
cd "$(dirname "$0")/../data/raw"
# Pinned so the model is reproducible; bump deliberately (MOZC_REF=master to track).
MOZC_REF=${MOZC_REF:-d766d25d253141eff71e25c593aaf753f8d91cee}
M=https://raw.githubusercontent.com/google/mozc/$MOZC_REF/src/data
for i in 0 1 2 3 4 5 6 7 8 9; do curl -sSfL -o mozc_dict0$i.txt $M/dictionary_oss/dictionary0$i.txt; done
curl -sSfL -o id.def $M/dictionary_oss/id.def
curl -sSfL -o connection_single_column.txt $M/dictionary_oss/connection_single_column.txt
curl -sSfL -o MOZC_DICTIONARY_README.txt $M/dictionary_oss/README.txt
curl -sSfL -o romanji-hiragana.tsv $M/preedit/romanji-hiragana.tsv
curl -sSfL -o boundary.def $M/rules/boundary.def
curl -sSfL -o segmenter.def $M/rules/segmenter.def
# English frequencies: case-fold alphabetic 1-grams, sum counts, keep top 150k
curl -sSfL -o frequency-all.txt.gz https://media.githubusercontent.com/media/hackerb9/gwordlist/master/frequency-all.txt.gz
zcat frequency-all.txt.gz | awk 'NR>1 && $2 ~ /^[A-Za-z]+$/ {w=tolower($2); c=$3; gsub(",","",c); print w"\t"c}' \
  | awk -F'\t' '{a[$1]+=$2} END{for(w in a) print w"\t"a[w]}' | sort -t$'\t' -k2,2nr | awk 'NR<=150000' > en_gbooks.tsv
rm -f frequency-all.txt.gz
# SCOWL / ESDB words up to size 70 (plain lowercase words and simple contractions)
# SCOWL lives on branch v2 (no master); pinned to a commit like Mozc.
SCOWL_REF=${SCOWL_REF:-1e5b7d3a72f47a71da5d28686c1dd4b397178485}
curl -sSfL -o scowl-pre.txt https://raw.githubusercontent.com/en-wl/wordlist/$SCOWL_REF/data/scowl-pre.txt
curl -sSfL -o SCOWL_COPYRIGHT.txt https://raw.githubusercontent.com/en-wl/wordlist/$SCOWL_REF/Copyright
awk -F': ' '/^[0-9]/ { split($1, h, " "); if (h[1]+0 <= 70) { split($2, w, " "); sub(/^!/, "", w[1]); if (w[1] ~ /^[a-z]+(\x27[a-z]+)?$/) print w[1] } }' scowl-pre.txt | sort -u > en_scowl70.txt
rm -f scowl-pre.txt
# CMU Pronouncing Dictionary (BSD) for the katakana ↔ English bridge
curl -sSfL -o cmudict.dict https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict
curl -sSfL -o CMUDICT_LICENSE.txt https://raw.githubusercontent.com/cmusphinx/cmudict/master/LICENSE
echo "done; now: cargo run --release --bin kswime-build-model -- data/raw data/model.bin --gen-bench tests/data"
