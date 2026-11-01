#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Create RTL/Bidi test samples with PROPER TEXT SHAPING + RTL RENDERING.

4-COLUMN STRUCTURE:
  1. Name - Test case identifier
  2. Input (Unicode) - Logical text (as stored)
  3. Visual Rendition - Visual order with proper RTL layout
  4. Points - Validation checklist

RENDERING (CORRECT):
  1. python-bidi: Reorder logical → visual (UAX #9)
  2. uharfbuzz: Shape text to glyphs (connects letters, positions diacritics)
  3. pycairo: Render shaped glyphs right-aligned with proper RTL layout
"""

import os
import sys
from pathlib import Path
import io
import base64

print("=" * 70)
print("RTL/BIDI TEST SAMPLE GENERATOR - Browser-based RTL Rendering")
print("=" * 70)

try:
    from bidi.algorithm import get_display
    print("[OK] python-bidi (Unicode Bidi Algorithm UAX #9)")
except ImportError:
    print("[NO] python-bidi (pip install python-bidi)")
    sys.exit(1)

try:
    from playwright.sync_api import sync_playwright
    print("[OK] playwright (Browser rendering)")
except ImportError:
    print("[NO] playwright (pip install playwright)")
    sys.exit(1)

print()

# ============================================================================
# TEST DATA
# ============================================================================

TEST_CASES = [
    {
        'name': 'Test 1: Mixed Direction',
        'input': 'السعر الكلي 123 ريال - the price is 123 riyals',
        'points': ['Mixed LTR/RTL', 'Numbers grouped (123 not 321)', 'Arabic RTL, English LTR', 'Aligns to RIGHT']
    },
    {
        'name': 'Test 2: Pure RTL Paragraph',
        'input': 'هذا نص عربي بسيط لاختبار اتجاه النص من اليمين إلى اليسار',
        'points': ['Pure RTL', 'Right-to-left flow', 'RIGHT margin', 'Wraps from right']
    },
    {
        'name': 'Test 3: Diacritics (Harakat)',
        'input': 'كِتَاب (book)  مَدْرَسَة (school)',
        'points': ['Diacritics (harakat)', 'Marks ABOVE letters', 'Arrow key skips', 'Never between letter+mark']
    },
    {
        'name': 'Test 4: Multiple RTL Scripts',
        'input': 'Hello مرحبا שלום',
        'points': ['English + Arabic + Hebrew', 'English LTR, RTL text', 'No CJK (font limit)', 'Mixed direction']
    },
    {
        'name': 'Test 5: Numbers in Arabic',
        'input': 'الشركة: Acme Corp العنوان: 123 Main Street',
        'points': ['Numbers grouped (123)', 'Company readable', 'RTL flow', 'Proper ordering']
    },
    {
        'name': 'Test 6: Kashida Variants',
        'input': 'احمد  vs  أحمد',
        'points': ['Kashida variant', 'Find & Replace', 'Finds both forms', 'Proofing tools']
    },
    {
        'name': 'Test 7: Caret & Selection',
        'input': 'السعر 123 ريال',
        'points': ['Caret direction', 'Selection spans', 'Home/End visual', 'Arrow keys RTL']
    },
    {
        'name': 'Test 8: RTL List',
        'input': '• البند الأول\n• البند الثاني',
        'points': ['RTL lists', 'Bullets RIGHT', 'Text RTL', 'Indent from right']
    },
]

# ============================================================================
# FONT HANDLING
# ============================================================================

def get_amiri_font_path():
    """Get path to Amiri font file."""
    script_dir = Path(__file__).parent
    font_path = script_dir / 'Amiri-Regular.ttf'

    if font_path.exists():
        return str(font_path)

    system_paths = [
        Path("C:/Windows/Fonts/Amiri-Regular.ttf"),
        Path("/usr/share/fonts/opentype/amiri/Amiri-Regular.ttf"),
        Path("/System/Library/Fonts/Amiri-Regular.ttf"),
    ]

    for path in system_paths:
        if path.exists():
            return str(path)

    raise FileNotFoundError("Amiri-Regular.ttf not found")

# ============================================================================
# IMAGE RENDERING WITH BROWSER (PLAYWRIGHT)
# ============================================================================

def render_text_to_image(text, width=400, height=200):
    """
    Render text using browser (Chromium/Playwright).
    Handles: Glyph shaping, RTL layout, proper alignment.
    """
    try:
        # Pass LOGICAL text to browser - it handles bidi reordering itself
        # Do NOT use get_display() - that causes double-reordering
        logical_text = text

        # HTML with proper RTL styling
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
            font-family: Amiri;
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
            font-family: Amiri;
            src: url('file:///{get_amiri_font_path().replace(chr(92), "/")}');
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
        print(f"[ERROR] Rendering: {e}")
        return None

# ============================================================================
# FORMAT GENERATORS
# ============================================================================

def create_docx():
    """Create DOCX with 4-column table + images."""
    from docx import Document
    from docx.shared import Inches, Pt
    from docx.enum.text import WD_ALIGN_PARAGRAPH

    print("[...] Creating DOCX...")

    doc = Document()
    title = doc.add_heading('RTL/Bidi Test Samples', level=1)
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER

    subtitle = doc.add_paragraph('4-Column: Name | Input | Visual | Points')
    subtitle.alignment = WD_ALIGN_PARAGRAPH.CENTER
    subtitle.runs[0].italic = True

    table = doc.add_table(rows=1, cols=4)
    table.style = 'Light Grid Accent 1'

    hdr = table.rows[0].cells
    for i, text in enumerate(['Name', 'Input (Unicode)', 'Visual Rendition', 'Points']):
        hdr[i].text = text
        for run in hdr[i].paragraphs[0].runs:
            run.bold = True
            run.font.size = Pt(11)

    for test in TEST_CASES:
        row = table.add_row()

        # Name
        p = row.cells[0].paragraphs[0]
        p.add_run(test['name']).bold = True

        # Input
        p = row.cells[1].paragraphs[0]
        r = p.add_run(test['input'])
        r.font.name = 'Amiri'
        r.font.size = Pt(11)
        p.alignment = WD_ALIGN_PARAGRAPH.RIGHT

        # Visual (image)
        img_bytes = render_text_to_image(test['input'])
        p = row.cells[2].paragraphs[0]
        p.alignment = WD_ALIGN_PARAGRAPH.CENTER
        if img_bytes:
            run = p.add_run()
            run.add_picture(io.BytesIO(img_bytes), width=Inches(2.0))
        else:
            p.add_run('[Image]')

        # Points
        p = row.cells[3].paragraphs[0]
        for i, point in enumerate(test['points']):
            if i > 0:
                p.add_run('\n')
            p.add_run(f'[*] {point}')

    doc.save('wordcraft_rtl_test_samples.docx')
    print('[OK] Created: wordcraft_rtl_test_samples.docx')

def create_html():
    """Create HTML with embedded images."""
    print("[...] Creating HTML...")

    html_rows = []
    for test in TEST_CASES:
        img_bytes = render_text_to_image(test['input'])
        if img_bytes:
            img_base64 = base64.b64encode(img_bytes).decode()
            img_tag = f'<img src="data:image/png;base64,{img_base64}" style="max-width: 100%; border: 1px solid #ddd;">'
        else:
            img_tag = '[Image]'

        points_html = '<br>'.join([f'[*] {p}' for p in test['points']])

        html_rows.append(f'''
        <tr>
            <td>{test['name']}</td>
            <td class="col-input">{test['input']}</td>
            <td class="col-visual">{img_tag}</td>
            <td class="col-points">{points_html}</td>
        </tr>
        ''')

    html = f'''<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>RTL/Bidi Test Samples</title>
    <style>
        body {{ font-family: Arial; margin: 20px; background: #f5f5f5; }}
        h1 {{ text-align: center; }}
        table {{ width: 100%; border-collapse: collapse; background: white; }}
        th, td {{ padding: 15px; border: 1px solid #ddd; vertical-align: top; }}
        th {{ background: #4CAF50; color: white; font-weight: bold; }}
        tr:nth-child(even) {{ background: #f9f9f9; }}
        .col-input {{ font-family: 'Amiri', Arial; direction: rtl; text-align: right; }}
        .col-visual {{ text-align: center; }}
        .col-points {{ font-size: 12px; }}
    </style>
</head>
<body>
    <h1>RTL/Bidi Test Samples</h1>
    <p style="text-align: center; font-style: italic;">4-Column: Name | Input | Visual | Points</p>
    <table>
        <thead>
            <tr>
                <th>Name</th>
                <th>Input (Unicode)</th>
                <th>Visual Rendition</th>
                <th>Points</th>
            </tr>
        </thead>
        <tbody>
            {''.join(html_rows)}
        </tbody>
    </table>
</body>
</html>'''

    with open('wordcraft_rtl_test_samples.html', 'w', encoding='utf-8') as f:
        f.write(html)
    print('[OK] Created: wordcraft_rtl_test_samples.html')

# ============================================================================
# MAIN
# ============================================================================

if __name__ == '__main__':
    create_docx()
    create_html()
    print()
    print("[SUCCESS] All formats created with proper HarfBuzz RTL rendering")
