#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

if [ -x "/Users/jameskaranja/.local/bin/tectonic" ]; then
  export PATH="/Users/jameskaranja/.local/bin:$PATH"
fi

if command -v tectonic >/dev/null 2>&1; then
  tectonic --keep-logs --keep-intermediates main.tex
elif command -v latexmk >/dev/null 2>&1; then
  latexmk -pdf main.tex
elif command -v pdflatex >/dev/null 2>&1 && command -v bibtex >/dev/null 2>&1; then
  pdflatex main.tex
  bibtex main
  pdflatex main.tex
  pdflatex main.tex
else
  echo "No supported TeX toolchain found. Install tectonic, latexmk, or pdflatex+bibtex." >&2
  exit 1
fi
