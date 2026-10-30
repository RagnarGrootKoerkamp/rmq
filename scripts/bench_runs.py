"""Locate run files. Runs live in their own git repo (https://github.com/lcjconte/rmq-data),
checked out at `runs/` in this repo, or wherever $BENCH_RUNS points. The first lookup pulls it
(skip with BENCH_RUNS_NO_SYNC=1); if the pull fails the local copy is used.

Scripts take run names relative to the runs dir, with or without `.jsonl`: `full_random`,
`full_random.jsonl`.
"""

import functools
import os
import subprocess
import sys
from pathlib import Path

DATA_REPO = "https://github.com/lcjconte/rmq-data.git"


@functools.cache
def runs_dir():
    if "BENCH_RUNS" in os.environ:
        path = Path(os.environ["BENCH_RUNS"]).expanduser()
    else:
        path = Path(__file__).resolve().parent.parent / "runs"
    if not path.is_dir():
        raise SystemExit(
            f"runs directory {path} does not exist. Clone the data repo there with\n"
            f"    git clone --filter=blob:none {DATA_REPO} {path}\n"
            "or set BENCH_RUNS to an existing clone."
        )
    sync(path)
    return path


def sync(path, timeout=10):
    if os.environ.get("BENCH_RUNS_NO_SYNC"):
        return
    try:
        subprocess.run(
            ["git", "-C", str(path), "pull", "--rebase", "--autostash", "-q"],
            timeout=timeout, check=True, capture_output=True, stdin=subprocess.DEVNULL,
            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        )
    except subprocess.CalledProcessError as e:
        print(f"warning: could not sync runs ({e.stderr.decode().strip()}); using local copy", file=sys.stderr)
    except (subprocess.SubprocessError, OSError) as e:
        print(f"warning: could not sync runs ({e}); using local copy", file=sys.stderr)


def run_path(name):
    """Path of run `name` inside the runs dir; `.jsonl` is appended if missing."""
    if not name.endswith(".jsonl"):
        name += ".jsonl"
    root = runs_dir()
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()):
        raise SystemExit(f"{name}: runs must be inside {root}")
    return path


def existing_run(name):
    path = run_path(name)
    if not path.is_file():
        raise SystemExit(f"no run {name!r} in {runs_dir()}")
    return path
