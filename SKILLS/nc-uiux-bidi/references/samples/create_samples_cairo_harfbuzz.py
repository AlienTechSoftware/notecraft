#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RTL/Bidi test samples using HarfBuzz shape + Cairo show_glyphs() render.
Traditional approach: shape -> show_glyphs
"""

import io
from pathlib import Path
import uharfbuzz as hb
import cairo
from docx import Document
from docx.shared import Inches, Pt, RGBColor
from docx.enum.text import WD_ALIGN_PARAGRAPH

TEST_CASES = [
    {'name': 'Test 1: Mixed Direction', 'input': 'السعر الكلي 123 ريال - the price is 123 riyals'},
    {'name': 'Test 2: Pure RTL', 'input': 'هذا نص عربي بسيط لاختبار اتجاه النص من اليمين إلى اليسار'},
    {'name': 'Test 3: Diacritics', 'input': 'كِتَاب (book)  مَدْرَسَة (school)'},
    {'name': 'Test 4: Multiple RTL', 'input': 'Hello مرحبا שלום'},
    {'name': 'Test 5: Numbers', 'input': 'الشركة: Acme Corp العنوان: 123 Main Street'},
    {'name': 'Test 6: Kashida', 'input': 'احمد  vs  أحمد'},
    {'name': 'Test 7: Selection', 'input': 'السعر 123 ريال'},
    {'name': 'Test 8: Lists', 'input': '• البند الأول\n• البند الثاني'},
]

def render_with_cairo_harfbuzz(text, font_path, font_name, width=400, height=200, font_size=24):
    """
    Render using HarfBuzz shape + Cairo show_glyphs().
    Traditional pipeline: shape with HarfBuzz, render glyphs with Cairo.
    """
    try:
        # Load font data for HarfBuzz
        with open(font_path, 'rb') as f:
            font_data = f.read()

        # HarfBuzz shaping
        face = hb.Face(font_data)
        font_hb = hb.Font(face)
        font_hb.scale = (font_size * 64, font_size * 64)

        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()
        hb.shape(font_hb, buf)

        infos = buf.glyph_infos
        positions = buf.glyph_positions

        print(f"  [{font_name}] Shaped: {len(infos)} glyphs")

        # Cairo surface and context
        surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, width, height)
        ctx = cairo.Context(surface)

        # White background
        ctx.set_source_rgb(1, 1, 1)
        ctx.paint()

        # Draw borders
        ctx.set_source_rgb(1, 0, 0)  # RED right
        ctx.rectangle(width - 4, 0, 4, height)
        ctx.fill()

        ctx.set_source_rgb(0, 0, 1)  # BLUE left
        ctx.rectangle(0, 0, 4, height)
        ctx.fill()

        # Load font with Cairo (using FreeType backend)
        ctx.select_font_face(font_name, cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        ctx.set_font_size(font_size)
        ctx.set_source_rgb(0, 0, 0)  # Black text

        # Get scaled font from context
        scaled_font = ctx.get_scaled_font()

        # Build glyph array for show_glyphs
        # Format: [(glyph_index, x, y), ...]
        glyphs = []
        x = width - 20  # Start from right
        y = font_size + 20

        for info, pos in zip(infos, positions):
            glyph_index = info.codepoint
            glyphs.append((glyph_index, x, y))
            x += pos.x_advance / 64.0

        # Render shaped glyphs using Cairo's show_glyphs
        try:
            ctx.show_glyphs(glyphs)
            print(f"  [{font_name}] Rendered {len(glyphs)} glyphs with show_glyphs()")
        except Exception as e:
            # Fallback: use show_text if show_glyphs fails
            print(f"  [{font_name}] show_glyphs() failed, using show_text(): {e}")
            ctx.move_to(width - 20, y)
            ctx.show_text(text)

        # Convert to PNG bytes
        png_bytes = io.BytesIO()
        surface.write_to_png(png_bytes)
        png_bytes.seek(0)

        return png_bytes.getvalue()

    except Exception as e:
        print(f"[ERROR] {font_name}: {e}")
        import traceback
        traceback.print_exc()
        return None

def create_cairo_harfbuzz_samples():
    """Create DOCX with Cairo show_glyphs() rendering."""
    doc = Document()

    # Title
    title = doc.add_heading('RTL/Bidi - HarfBuzz + Cairo show_glyphs()', level=1)
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER

    subtitle = doc.add_paragraph('HarfBuzz shape -> Cairo show_glyphs() render (traditional approach)')
    subtitle.alignment = WD_ALIGN_PARAGRAPH.CENTER
    subtitle.runs[0].italic = True

    fonts = {
        'Amiri': 'Amiri-Regular.ttf',
        'Noto Sans Arabic': 'NotoSansArabic-Regular.ttf',
        'Liberation Sans': 'LiberationSans-Regular.ttf',
    }

    for font_name, font_file in fonts.items():
        font_path = Path(__file__).parent / font_file

        if not font_path.exists():
            print(f"[SKIP] {font_name}: {font_path} not found")
            continue

        # Section heading
        heading = doc.add_heading(f'Font: {font_name}', level=2)
        heading.runs[0].font.color.rgb = RGBColor(0, 102, 204)

        print(f"\n[{font_name}]")

        # Create table
        table = doc.add_table(rows=1, cols=3)
        table.style = 'Light Grid Accent 1'

        # Header
        hdr = table.rows[0].cells
        for i, text in enumerate(['Name', 'Input', 'HarfBuzz + Cairo']):
            hdr[i].text = text
            for run in hdr[i].paragraphs[0].runs:
                run.bold = True

        # Test cases
        for test in TEST_CASES:
            row = table.add_row()

            # Name
            p = row.cells[0].paragraphs[0]
            r = p.add_run(test['name'])
            r.bold = True

            # Input
            p = row.cells[1].paragraphs[0]
            p.add_run(test['input'])
            p.alignment = WD_ALIGN_PARAGRAPH.RIGHT

            # HarfBuzz + Cairo
            img_bytes = render_with_cairo_harfbuzz(test['input'], str(font_path), font_name)
            p = row.cells[2].paragraphs[0]
            p.alignment = WD_ALIGN_PARAGRAPH.CENTER
            if img_bytes:
                run = p.add_run()
                run.add_picture(io.BytesIO(img_bytes), width=Inches(1.8))

        doc.add_page_break()

    doc.save('wordcraft_rtl_test_samples_cairo_harfbuzz.docx')
    print("\n[OK] Created: wordcraft_rtl_test_samples_cairo_harfbuzz.docx")

if __name__ == '__main__':
    print("=" * 70)
    print("HarfBuzz Shape + Cairo show_glyphs() Render")
    print("=" * 70)
    print("\nPipeline: HarfBuzz shape -> Cairo show_glyphs()...")

    create_cairo_harfbuzz_samples()

    print("\n[SUCCESS] Cairo HarfBuzz rendering complete")
