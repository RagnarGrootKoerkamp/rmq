#!/usr/bin/env bash
# Download the T2T-CHM13v2.0 human genome (GCF_009914755.1) to ncbi_dataset/genome.fna,
# the input read by src/main.rs. Run from anywhere; does nothing if the file already exists.
#
#     scripts/fetch_genome.sh
set -euo pipefail

URL=https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/009/914/755/GCF_009914755.1_T2T-CHM13v2.0/GCF_009914755.1_T2T-CHM13v2.0_genomic.fna.gz
MD5=9621f8e9e8cbf0d2c020677714195ac3

cd "$(dirname "$0")/.."
out=ncbi_dataset/genome.fna

if [[ -e $out ]]; then
    echo "$out already exists" >&2
    exit 0
fi

mkdir -p ncbi_dataset
curl -fL --retry 3 "$URL" | gunzip > "$out.tmp"

if command -v md5sum > /dev/null; then
    sum=$(md5sum "$out.tmp" | cut -d' ' -f1)
else
    sum=$(md5 -q "$out.tmp")
fi
if [[ $sum != "$MD5" ]]; then
    echo "md5 mismatch: got $sum, expected $MD5" >&2
    rm "$out.tmp"
    exit 1
fi

mv "$out.tmp" "$out"
echo "wrote $out"
