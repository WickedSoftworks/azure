"""Subset Iosevka to the glyphs this interface actually draws.

Iosevka's latin subset ships ~960 KB per weight because it carries the full
programming-symbol and ligature set. Azure draws ASCII plus a dozen marks, so
subsetting takes the four weights from 3.78 MB to ~24 KB with no visible
change. Re-run after adding any new glyph (a new arrow, a new unit symbol):

    python scripts/subset-fonts.py
"""
import pathlib
import subprocess
import sys

SRC = pathlib.Path("node_modules/@fontsource/iosevka/files")
OUT = pathlib.Path("src/assets/fonts")
WEIGHTS = (400, 500, 700, 800)
UNICODES = (
    "U+0020-007E,U+00A0,U+00B0,U+00B1,U+00B7,U+00D7,"
    "U+2013,U+2014,U+2018-201D,U+2022,U+2026,U+2190,U+2192,U+2264,U+2265"
)


def main() -> int:
    if not SRC.exists():
        print("Run `bun install` first: @fontsource/iosevka is missing.", file=sys.stderr)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    for weight in WEIGHTS:
        source = SRC / f"iosevka-latin-{weight}-normal.woff2"
        target = OUT / f"iosevka-{weight}.woff2"
        subprocess.run(
            [
                sys.executable, "-m", "fontTools.subset", str(source),
                f"--unicodes={UNICODES}",
                "--layout-features=",  # no programming ligatures in a readout
                "--no-hinting",
                "--desubroutinize",
                "--drop-tables+=GSUB,GPOS",
                "--flavor=woff2",
                f"--output-file={target}",
            ],
            check=True,
        )
        print(f"{weight}: {source.stat().st_size // 1024} KB -> {target.stat().st_size // 1024} KB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
