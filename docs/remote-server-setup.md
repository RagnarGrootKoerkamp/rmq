# Running benchmarks on a remote server

Benchmark results live in their own repo, https://github.com/lcjconte/rmq-data, checked out at
`runs/` inside this repo. `scripts/run_and_publish.sh` runs a benchmark into that checkout,
commits the new run files and pushes them, so they show up locally the next time a plotting
script runs (the scripts pull `runs/` before loading).

The server only needs write access to the data repo. These steps are done once per server, by hand.

## 1. Deploy key for the data repo

On the server:

```sh
ssh-keygen -t ed25519 -N "" -C "rmq-data deploy key ($(hostname -s))" -f ~/.ssh/rmq-data-deploy
cat ~/.ssh/rmq-data-deploy.pub
```

On GitHub, open lcjconte/rmq-data → Settings → Deploy keys → Add deploy key, paste the public key
and tick **Allow write access**. A deploy key grants access to this one repo only.

## 2. SSH host alias

Add to `~/.ssh/config` on the server, so the data repo uses the deploy key and nothing else does:

```
Host github-rmq-data
    HostName github.com
    User git
    IdentityFile ~/.ssh/rmq-data-deploy
    IdentitiesOnly yes
```

Check it with `ssh -T github-rmq-data`; GitHub should greet you with the repo name.

## 3. Clone both repos

```sh
git clone https://github.com/RagnarGrootKoerkamp/rmq.git
cd rmq
git clone --filter=blob:none git@github-rmq-data:lcjconte/rmq-data.git runs
```

`--filter=blob:none` fetches run files only as they are checked out, so old revisions of large
runs aren't downloaded.

## 4. Commit identity in the data repo

```sh
git -C runs config user.name "rmq bench ($(hostname -s))"
git -C runs config user.email "you@example.com"
```

## 5. Run benchmarks

Benchmarks take a long time, so run them in `tmux` (or `nohup`) to survive a dropped SSH session:

```sh
tmux new -s bench
scripts/run_and_publish.sh sweep main                 # all n from 1e3 to 1e7
scripts/run_and_publish.sh big main --min-n 1e8 --max-n 1e8
scripts/run_and_publish.sh ranges ranges
# detach with Ctrl-b d, reattach with: tmux attach -t bench
```

or

```sh
nohup scripts/run_and_publish.sh sweep main > bench.log 2>&1 &
```

The run is named `<date>_<time>_<label>_<host>`. If the benchmark fails nothing is committed.
If pushing fails five times the run stays committed in `runs/`; push it later with
`git -C runs pull --rebase && git -C runs push`.

## After the data repo is squashed

Squashing the data repo's history and force-pushing it makes every existing clone diverge. On each
clone (the server and every laptop), first make sure there are no unpushed runs. Do this
*before* fetching, since after the fetch `origin/main` points at the new, unrelated history:

```sh
git -C runs status --short                    # must print nothing
git -C runs log --oneline origin/main..main   # must print nothing
```

If either prints something, copy those run files aside, reset as below, then copy them back and
commit them on top of the new history. Once both print nothing:

```sh
git -C runs fetch
git -C runs reset --hard origin/main
```
