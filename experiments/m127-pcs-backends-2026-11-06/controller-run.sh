#!/bin/bash
set -u
cd /home/zjl/HyperFold
export FOURQ_ROOT=/home/zjl/zkp-research/reference/FourQlib
python3 benchmarks/m127-pcs-compare/run.py --binary benchmarks/m127-pcs-compare/target-native/release/hyperfold-m127-pcs-compare --root /home/zjl/zkp-research/experiments/hyperfold-bench/m127-pcs-backends-2026-11-06/data
bench_result=$?
printf '%s\n' "$bench_result" > /home/zjl/zkp-research/experiments/hyperfold-bench/m127-pcs-backends-2026-11-06/exit-status
exit "$bench_result"
