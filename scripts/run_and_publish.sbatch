#!/usr/bin/env bash
# Slurm wrapper around scripts/run_and_publish.sh. Submit from the repo root; the arguments are
# passed through unchanged:
#
#     sbatch scripts/run_and_publish.sbatch <label> main [--min-n N] [--max-n N]
#     sbatch scripts/run_and_publish.sbatch <label> ranges
#     sbatch scripts/run_and_publish.sbatch <label> genome
#
# Override any #SBATCH line on the command line, e.g. `sbatch -p bench -t 2-00:00:00 ...`.
# Build (and for genome, scripts/fetch_genome.sh) on the login node first: the job runs cargo
# offline, since compute nodes often have no internet access. The log goes to
# slurm-rmq-bench-<jobid>.out in the submit dir.
#
# The benchmark is single-threaded and measures latency, so the job takes a whole node and pins
# the process to one core. `genome` is left unpinned so suffix-array construction can use every
# core. Set BENCH_NO_PIN=1 to disable pinning.
#
#SBATCH --job-name=rmq-bench
#SBATCH --output=slurm-%x-%j.out
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --exclusive
#SBATCH --mem=0
#SBATCH --time=24:00:00
#SBATCH --partition=hellman
set -euo pipefail

# sbatch runs a copy of this script from the spool dir, so locate the repo via the submit dir.
CODE="${SLURM_SUBMIT_DIR:-$(pwd)}"
if [[ ! -x $CODE/scripts/run_and_publish.sh ]]; then
    echo "submit from the rmq repo root ($CODE has no scripts/run_and_publish.sh)" >&2
    exit 2
fi
cd "$CODE"

# Batch shells are non-login, so rustup's PATH setup may not have run.
if ! command -v cargo > /dev/null && [[ -f $HOME/.cargo/env ]]; then
    source "$HOME/.cargo/env"
fi
export CARGO_NET_OFFLINE=true
export OMP_NUM_THREADS="${SLURM_CPUS_ON_NODE:-$(nproc)}"

echo "job $SLURM_JOB_ID on $(hostname -s), $(date -Is)"
echo "code $(git rev-parse --short HEAD)$(git diff --quiet || echo '-dirty'), args: $*"

cmd=(scripts/run_and_publish.sh "$@")
if [[ -z ${BENCH_NO_PIN:-} && ${2:-} != genome ]]; then
    # Last CPU in the job's allowed set; core 0 tends to take most interrupts.
    cpu=$(taskset -pc $$ | sed 's/.*: //' | tr ',' '\n' | tail -1 | sed 's/.*-//')
    echo "pinning to cpu $cpu"
    cmd=(taskset -c "$cpu" "${cmd[@]}")
fi

"${cmd[@]}"
echo "done $(date -Is)"
