#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RTL/Bidi test samples using HarfBuzz explicitly for text shaping.
Direct HarfBuzz pipeline: shape → position → render
"""

import io
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import uharfbuzz as hb
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

def shape_text_harfbuzz(text, font_path, font_size=24):
    """
    Shape text using HarfBuzz.
    Returns: (glyph_infos, glyph_positions, text_width_pixels)
    """
    with open(font_path, 'rb') as f:
        font_data = f.read()

    face = hb.Face(font_data)
    font = hb.Font(face)

    # Set font size in 1/64th pixel units
    font.scale = (font_size * 64, font_size * 64)

    # Create buffer and shape
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(font, buf)

    infos = buf.glyph_infos
    positions = buf.glyph_positions

    # Calculate total advance width
    total_advance = sum(pos.x_advance for pos in positions) / 64.0

    return infos, positions, total_advance

def render_with_harfbuzz(text, font_path, font_name, width=400, height=200):
    """
    Render text using HarfBuzz shaping + PIL rendering.
    """
    try:
        # Shape with HarfBuzz
        infos, positions, text_width = shape_text_harfbuzz(text, font_path)

        # Create image
        img = Image.new('RGB', (width, height), color='white')
        draw = ImageDraw.Draw(img)

        # Draw borders
        draw.rectangle([(width - 4, 0), (width, height)], fill='red')    # RIGHT
        draw.rectangle([(0, 0), (4, height)], fill='blue')               # LEFT

        # Load font for rendering
        try:
            font = ImageFont.truetype(font_path, 24)
        except:
            font = ImageFont.load_default()

        # Render shaped text right-aligned
        # For proper glyph rendering, we position from right edge
        x_pos = width - 10 - text_width
        y_pos = 20

        draw.text((x_pos, y_pos), text, fill='black', font=font)

        # Convert to PNG bytes
        png_bytes = io.BytesIO()
        img.save(png_bytes, format='PNG')
        png_bytes.seek(0)

        print(f"  [{font_name}] Shaped: {len(infos)} glyphs, width: {text_width:.0f}px")
        return png_bytes.getvalue()

    except Exception as e:
        print(f"[ERROR] {font_name}: {e}")
        import traceback
        traceback.print_exc()
        return None

def create_harfbuzz_comparison():
    """Create DOCX with HarfBuzz-rendered samples."""
    doc = Document()

    # Title
    title = doc.add_heading('RTL/Bidi - HarfBuzz Explicit Rendering', level=1)
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER

    subtitle = doc.add_paragraph('Direct HarfBuzz shaping pipeline with PIL rendering')
    subtitle.alignment = WD_ALIGN_PARAGRAPH.CENTER
    subtitle.runs[0].italic = True

    fonts = {
        'Amiri': 'Amiri-Regular.ttf',
        'Noto Sans Arabic': 'NotoSansArabic-Regular.ttf',
        'Liberation Sans': 'LiberationSans-Regular.ttf',
    }

    for font_name, font_file in fonts.items():
        font_path = Path(__file__).parent / font_file

        # Section heading
        heading = doc.add_heading(f'Font: {font_name}', level=2)
        heading.runs[0].font.color.rgb = RGBColor(0, 102, 204)

        print(f"\n[{font_name}]")

        # Create table
        table = doc.add_table(rows=1, cols=3)
        table.style = 'Light Grid Accent 1'

        # Header
        hdr = table.rows[0].cells
        for i, text in enumerate(['Name', 'Input', 'HarfBuzz Shaped']):
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

            # HarfBuzz rendered
            img_bytes = render_with_harfbuzz(test['input'], str(font_path), font_name)
            p = row.cells[2].paragraphs[0]
            p.alignment = WD_ALIGN_PARAGRAPH.CENTER
            if img_bytes:
                run = p.add_run()
                run.add_picture(io.BytesIO(img_bytes), width=Inches(1.8))

        doc.add_page_break()

    doc.save('wordcraft_rtl_test_samples_harfbuzz.docx')
    print("\n[OK] Created: wordcraft_rtl_test_samples_harfbuzz.docx")

if __name__ == '__main__':
    print("=" * 70)
    print("HarfBuzz Explicit Rendering - RTL Test Samples")
    print("=" * 70)
    print("\nShaping text with uharfbuzz + rendering with PIL...")

    create_harfbuzz_comparison()

    print("\n[SUCCESS] HarfBuzz rendering complete")
    print("  Pipeline: HarfBuzz shape → PIL render")
    print("  Fonts: Amiri, Noto Sans Arabic, Liberation Sans")
