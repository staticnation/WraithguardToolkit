#!/bin/sh
# Remakes viewer-shell/meshopt-port/tests/oracle_values.tsv and simplify_values.tsv from
# meshoptimizer 1.3 itself.
#   tools/meshopt_oracle/run.sh                 the test set (writes the .tsv)
#   tools/meshopt_oracle/run.sh <nif folder>    a corpus: prints where its .tsv went
# Needs git, a C++ compiler and cargo. meshoptimizer is cloned to a temporary folder,
# never into the repository.
set -e
here=$(cd "$(dirname "$0")" && pwd)
crate="$here/../../viewer-shell/meshopt-port"
work=${TMPDIR:-/tmp}/meshopt_oracle
mkdir -p "$work"
[ -d "$work/mo" ] || git clone -q --depth 1 --branch v1.3 https://github.com/zeux/meshoptimizer "$work/mo"
nice -n 19 c++ -O1 -std=c++11 -ffp-contract=off -I"$work/mo/src" "$here/oracle.cpp" "$work"/mo/src/*.cpp -o "$work/oracle"
nice -n 19 c++ -O1 -std=c++11 -ffp-contract=off -I"$work/mo/src" "$here/oracle_simplify.cpp" "$work"/mo/src/*.cpp -o "$work/oracle_simplify"
(cd "$crate" && cargo run -q --example dump_parts -- "$work/parts.bin" "$@")
if [ $# -eq 0 ]; then
  "$work/oracle" "$work/parts.bin" > "$crate/tests/oracle_values.tsv"
  "$work/oracle_simplify" "$work/parts.bin" > "$crate/tests/simplify_values.tsv"
  echo "wrote $crate/tests/oracle_values.tsv and simplify_values.tsv"
else
  "$work/oracle" "$work/parts.bin" > "$work/corpus.tsv"
  "$work/oracle_simplify" "$work/parts.bin" > "$work/corpus_simplify.tsv"
  echo "MESHOPT_PORT_CORPUS=$1 MESHOPT_PORT_CORPUS_ORACLE=$work/corpus.tsv MESHOPT_PORT_CORPUS_SIMPLIFY=$work/corpus_simplify.tsv cargo test --test oracle --test simplify"
fi
