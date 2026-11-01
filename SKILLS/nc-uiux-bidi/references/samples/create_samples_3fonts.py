#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Generate RTL/Bidi test samples comparing 3 fonts in one DOCX.
Fonts: Amiri, Noto Sans Arabic, Liberation Sans
"""

import io
import base64
from bidi.algorithm import get_display
from docx import Document
from docx.shared import Inches, Pt, RGBColor
from docx.enum.text import WD_ALIGN_PARAGRAPH
from playwright.sync_api import sync_playwright

TEST_CASES = [
    {'name': 'Test 1: Mixed Direction', 'input': 'السعر الكلي 123 ريال - the price is 123 riyals'},
    {'name': 'Test 2: Pure RTL Paragraph', 'input': 'هذا نص عربي بسيط لاختبار اتجاه النص من اليمين إلى اليسار'},
    {'name': 'Test 3: Diacritics', 'input': 'كِتَاب (book)  مَدْرَسَة (school)'},
    {'name': 'Test 4: Multiple RTL Scripts', 'input': 'Hello مرحبا שלום'},
    {'name': 'Test 5: Numbers in Arabic', 'input': 'الشركة: Acme Corp العنوان: 123 Main Street'},
    {'name': 'Test 6: Kashida Variants', 'input': 'احمد  vs  أحمد'},
    {'name': 'Test 7: Caret & Selection', 'input': 'السعر 123 ريال'},
    {'name': 'Test 8: RTL List', 'input': '• البند الأول\n• البند الثاني'},
]

FONTS = {
    'Amiri': 'C:/dev/p-NoteCraft/.repos/o-NoteCraft/SKILLS/nc-uiux-bidi/references/samples/Amiri-Regular.ttf',
    'Noto Sans Arabic': 'C:/Windows/Fonts/NotoSansArabic-Regular.ttf',
    'Liberation Sans': 'C:/Windows/Fonts/LiberationSans-Regular.ttf',
}

def render_with_font(text, font_name, font_path, width=400, height=200):
    """Render text with specified font using browser."""
    try:
        logical_text = text
        html = f'''<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ margin: 0; padding: 0; }}
        .container {{
            width: {width}px;
            height: {height}px;
            display: block;
            padding: 20px 10px;
            font-family: "{font_name}";
            font-size: 24px;
            direction: rtl;
            text-align: right;
            border-left: 4px solid blue;
            border-right: 4px solid red;
            box-sizing: border-box;
            white-space: pre-wrap;
            word-wrap: break-word;
            overflow: hidden;
        }}
        @font-face {{
            font-family: "{font_name}";
            src: url('file:///{font_path.replace(chr(92), "/")}');
        }}
    </style>
</head>
<body>
    <div class="container">{logical_text}</div>
</body>
</html>'''

        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page(viewport={'width': width, 'height': height})
            page.set_content(html)
            screenshot = page.screenshot()
            browser.close()
            return screenshot
    except Exception as e:
        print(f"[ERROR] {font_name}: {e}")
        return None

def create_docx_3fonts():
    """Create DOCX with 3 font comparison sections."""
    doc = Document()

    title = doc.add_heading('RTL/Bidi Test Samples - Font Comparison', level=1)
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER

    subtitle = doc.add_paragraph('Comparing 3 fonts: Amiri | Noto Sans Arabic | Liberation Sans')
    subtitle.alignment = WD_ALIGN_PARAGRAPH.CENTER
    subtitle.runs[0].italic = True

    # Create section for each font
    for font_name, font_path in FONTS.items():
        # Font section heading
        heading = doc.add_heading(f'Font: {font_name}', level=2)
        heading.runs[0].font.color.rgb = RGBColor(0, 102, 204)

        # Create table for this font
        table = doc.add_table(rows=1, cols=3)
        table.style = 'Light Grid Accent 1'

        # Header
        hdr = table.rows[0].cells
        for i, text in enumerate(['Name', 'Input (Unicode)', 'Visual Rendition']):
            hdr[i].text = text
            for run in hdr[i].paragraphs[0].runs:
                run.bold = True
                run.font.size = Pt(11)

        # Test cases
        for test in TEST_CASES:
            row = table.add_row()

            # Name
            p = row.cells[0].paragraphs[0]
            r = p.add_run(test['name'])
            r.bold = True
            r.font.size = Pt(10)

            # Input
            p = row.cells[1].paragraphs[0]
            r = p.add_run(test['input'])
            r.font.name = font_name
            r.font.size = Pt(10)
            p.alignment = WD_ALIGN_PARAGRAPH.RIGHT

            # Visual (image)
            img_bytes = render_with_font(test['input'], font_name, font_path)
            p = row.cells[2].paragraphs[0]
            p.alignment = WD_ALIGN_PARAGRAPH.CENTER
            if img_bytes:
                run = p.add_run()
                run.add_picture(io.BytesIO(img_bytes), width=Inches(1.8))
            else:
                p.add_run('[Image]')

        # Add page break between font sections
        doc.add_page_break()

    doc.save('wordcraft_rtl_test_samples_3fonts.docx')
    print('[OK] Created: wordcraft_rtl_test_samples_3fonts.docx')

if __name__ == '__main__':
    print("=" * 70)
    print("RTL/BIDI TEST SAMPLES - 3 Font Comparison")
    print("=" * 70)
    print()
    create_docx_3fonts()
    print()
    print("[SUCCESS] 3-font comparison DOCX ready")
    print("  Fonts: Amiri | Noto Sans Arabic | Liberation Sans")
    print("  Tests: 8 cases per font")
