#!/usr/bin/env python3
"""Build the two static UI fonts from the Noto Sans SC variable font.

egui cannot select a weight inside a variable font and renders its default
master, which for Noto Sans SC is Thin (wght 100). This script instantiates:

  assets/NotoSansSC-Regular.ttf   wght 400, every glyph (body text, file names)
  assets/NotoSansSC-SemiBold.ttf  wght 600, subset for headings and buttons:
                                  ASCII, Latin-1, common punctuation, all of
                                  GB 2312 and every character used in src/

OpenType layout tables are dropped because egui does not shape text.
Run again after adding UI text with characters outside GB 2312; the test
`heading_font_covers_every_ui_character` reports when that is needed.

Requires fontTools:  python3 -m pip install fonttools
Usage:               python3 scripts/make_fonts.py [path/to/NotoSansSC-VF.ttf]
"""
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'assets/NotoSansSC.ttf'
DROP = ['vhea', 'vmtx', 'BASE', 'STAT', 'GSUB', 'GPOS', 'GDEF', 'DSIG']


def gb2312():
    chars = set()
    for hi in range(0xA1, 0xF8):
        for lo in range(0xA1, 0xFF):
            try:
                chars.add(ord(bytes([hi, lo]).decode('gb2312')))
            except UnicodeDecodeError:
                pass
    return chars


def source_chars():
    chars = set()
    for path in (ROOT / 'src').rglob('*.rs'):
        chars |= {ord(c) for c in path.read_text(encoding='utf-8') if not c.isspace()}
    return chars


def build(weight, unicodes, output):
    # Keep the source timestamp so regenerating produces identical files.
    source = TTFont(SOURCE, recalcTimestamp=False)
    font = instancer.instantiateVariableFont(source, {'wght': weight}, updateFontNames=True)
    options = subset.Options()
    options.layout_features = []
    options.drop_tables += DROP
    options.hinting = False
    options.name_IDs = ['*']
    options.name_languages = ['*']
    options.notdef_outline = True
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=unicodes if unicodes is not None else font.getBestCmap().keys())
    subsetter.subset(font)
    font.save(output)
    print(f'{output.relative_to(ROOT)}: {output.stat().st_size / 1024 / 1024:.2f} MiB, '
          f'{font["maxp"].numGlyphs} glyphs')


def main():
    if 'fvar' not in TTFont(SOURCE, lazy=True):
        raise SystemExit(f'{SOURCE} is not a variable font')
    build(400, None, ROOT / 'assets/NotoSansSC-Regular.ttf')
    ranges = [(0x20, 0x7F), (0xA0, 0x100), (0x2000, 0x2070), (0x2190, 0x2200),
              (0x2460, 0x2500), (0x25A0, 0x2600), (0x3000, 0x3040), (0xFF00, 0xFFF0)]
    common = {c for a, b in ranges for c in range(a, b)}
    build(600, common | gb2312() | source_chars(), ROOT / 'assets/NotoSansSC-SemiBold.ttf')


if __name__ == '__main__':
    main()
