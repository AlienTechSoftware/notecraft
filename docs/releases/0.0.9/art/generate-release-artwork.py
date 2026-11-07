#!/usr/bin/env python3
"""Build the v0.0.9 release-preview GIF from the supplied artwork background.

Usage: python docs/releases/0.0.9/art/generate-release-artwork.py
Requires Pillow, as does the shared release-notes GIF generator.
"""

from pathlib import Path
from PIL import Image, ImageDraw, ImageFont


ART_DIR = Path(__file__).resolve().parent
RELEASE_DIR = ART_DIR.parent
BACKGROUND = ART_DIR / "0.0.9.jfif"
OUTPUT = RELEASE_DIR / "RELEASE_NOTES.gif"
FRAME_DURATION_MS = 3600

PURPLE = (74, 39, 112, 255)
VIOLET = (126, 68, 181, 255)
INK = (55, 47, 64, 255)
MUTED = (102, 91, 112, 255)
LINE = (190, 165, 216, 255)
WHITE = (255, 255, 255, 238)
GOLD = (220, 155, 42, 255)


def get_font(size, bold=False, mono=False):
    if mono:
        candidates = [
            Path(r"C:\Windows\Fonts\consola.ttf"),
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            Path("/System/Library/Fonts/Menlo.ttc"),
        ]
    else:
        filename = "segoeuib.ttf" if bold else "segoeui.ttf"
        candidates = [
            Path(r"C:\Windows\Fonts") / filename,
            Path("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            Path("/System/Library/Fonts/Helvetica.ttc"),
        ]
    for candidate in candidates:
        if candidate.exists():
            return ImageFont.truetype(str(candidate), size)
    return ImageFont.load_default()


def text_width(draw, text, face):
    box = draw.textbbox((0, 0), text, font=face)
    return box[2] - box[0]


def centered(draw, text, y, face, color, width):
    box = draw.textbbox((0, 0), text, font=face)
    x = (width - (box[2] - box[0])) // 2 - box[0]
    draw.text((x, y), text, font=face, fill=color)


def centered_at(draw, text, y, face, color, center_x):
    box = draw.textbbox((0, 0), text, font=face)
    x = center_x - (box[2] - box[0]) // 2 - box[0]
    draw.text((x, y), text, font=face, fill=color)


def wrapped(draw, text, face, max_width):
    words = text.split()
    lines = []
    line = ""
    for word in words:
        candidate = f"{line} {word}".strip()
        if line and text_width(draw, candidate, face) > max_width:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines


def centered_paragraph(draw, text, y, face, color, width, max_width, line_gap=9):
    lines = wrapped(draw, text, face, max_width)
    line_height = face.size + line_gap
    for line in lines:
        centered(draw, line, y, face, color, width)
        y += line_height
    return y


def frame_base(label, title):
    image = Image.open(BACKGROUND).convert("RGBA")
    width, _ = image.size
    draw = ImageDraw.Draw(image, "RGBA")

    label_font = get_font(19, bold=True)
    label_w = text_width(draw, label, label_font)
    pill = (width // 2 - label_w // 2 - 17, 119, width // 2 + label_w // 2 + 17, 155)
    draw.rounded_rectangle(pill, radius=18, fill=(246, 239, 252, 255), outline=LINE, width=1)
    centered(draw, label, 125, label_font, VIOLET, width)
    centered(draw, title, 174, get_font(42, bold=True), PURPLE, width)
    return image, draw


def draw_checklist(draw, width):
    items = [
        "Create a notebook",
        "Add sections and pages",
        "Choose the default format",
        "Try attaching a picture",
    ]
    face = get_font(25)
    y = 240
    for item in items:
        x = width // 2 - 245
        draw.rounded_rectangle((x, y + 2, x + 28, y + 30), radius=5, outline=VIOLET, width=2)
        draw.line((x + 6, y + 16, x + 12, y + 22), fill=VIOLET, width=3)
        draw.line((x + 12, y + 22, x + 23, y + 8), fill=VIOLET, width=3)
        draw.text((x + 44, y), item, font=face, fill=INK)
        y += 48


def draw_format_table(draw, width):
    x0, y0 = width // 2 - 440, 246
    col_widths = [255, 245, 380]
    headers = ["Format", "File", "Good for"]
    rows = [
        ["HTML", "index.html", "Rich layout"],
        ["CommonMark + GFM", "index.md", "Clean diffs and merges"],
    ]
    row_h = 58
    face = get_font(22)
    bold = get_font(22, bold=True)
    for row_index, row in enumerate([headers] + rows):
        x = x0
        y = y0 + row_index * row_h
        for col_index, value in enumerate(row):
            cell_w = col_widths[col_index]
            fill = (239, 228, 248, 255) if row_index == 0 else (255, 255, 255, 245)
            draw.rectangle((x, y, x + cell_w, y + row_h), fill=fill, outline=LINE, width=1)
            draw.text((x + 14, y + 15), value, font=bold if row_index == 0 else face, fill=PURPLE if row_index == 0 else INK)
            x += cell_w


def draw_folder_cards(draw, width):
    cards = [
        "A page and its media\nmove together.",
        "Sub-pages nest\nwithout a limit.",
        "A backup is\na copy.",
    ]
    card_w, gap = 292, 22
    total = card_w * len(cards) + gap * (len(cards) - 1)
    x = (width - total) // 2
    for text in cards:
        draw.rounded_rectangle((x, 265, x + card_w, 377), radius=18, fill=(255, 255, 255, 248), outline=LINE, width=2)
        for line_index, line in enumerate(text.split("\n")):
            centered_at(draw, line, 294 + line_index * 34, get_font(23, bold=True), INK, x + card_w // 2)
        x += card_w + gap


def draw_feature_pills(draw, width):
    features = ["Infinite canvas", "Android preview", "Language work"]
    face = get_font(19, bold=True)
    gap = 14
    widths = [text_width(draw, feature, face) + 34 for feature in features]
    x = (width - sum(widths) - gap * (len(widths) - 1)) // 2
    y = 400
    for feature, pill_width in zip(features, widths):
        draw.rounded_rectangle((x, y, x + pill_width, y + 39), radius=19, fill=(246, 239, 252, 255), outline=LINE, width=1)
        centered_at(draw, feature, y + 9, face, VIOLET, x + pill_width // 2)
        x += pill_width + gap


def draw_equation_and_graph(draw, width):
    centered(draw, "E = mc^2", 244, get_font(30, bold=True), INK, width)
    centered(draw, "int_0^1 x^2 dx = 1/3", 294, get_font(25), MUTED, width)

    box_w, box_h = 160, 42
    page_x, page_y = width // 2 - box_w // 2, 346
    child_y = 407
    child_xs = (width // 2 - 265, width // 2 + 105)
    page_mid = page_x + box_w // 2
    for child_x in child_xs:
        child_mid = child_x + box_w // 2
        draw.line((page_mid, page_y + box_h, child_mid, child_y), fill=GOLD, width=3)
        draw.polygon(
            [(child_mid - 6, child_y - 9), (child_mid, child_y), (child_mid + 6, child_y - 9)],
            fill=GOLD,
        )
    boxes = [(page_x, page_y, "Page"), (child_xs[0], child_y, "SubPage"), (child_xs[1], child_y, "Assets")]
    label_face = get_font(19, bold=True)
    for x, y, label in boxes:
        draw.rounded_rectangle((x, y, x + box_w, y + box_h), radius=11, fill=(246, 239, 252, 255), outline=VIOLET, width=2)
        centered_at(draw, label, y + 10, label_face, PURPLE, x + box_w // 2)


def draw_code_and_strike(draw, width):
    code_box = (width // 2 - 330, 248, width // 2 + 330, 355)
    draw.rounded_rectangle(code_box, radius=14, fill=(49, 39, 65, 255), outline=VIOLET, width=2)
    mono = get_font(24, mono=True)
    draw.text((code_box[0] + 26, code_box[1] + 34), 'let idea = "keep it simple";', font=mono, fill=(248, 239, 255, 255))

    old_text = "old wording"
    new_text = "revised wording"
    face = get_font(23, bold=True)
    old_x = width // 2 - 185 - text_width(draw, old_text, face) // 2
    new_x = width // 2 + 185 - text_width(draw, new_text, face) // 2
    y = 388
    draw.text((old_x, y), old_text, font=face, fill=MUTED)
    old_box = draw.textbbox((old_x, y), old_text, font=face)
    strike_y = (old_box[1] + old_box[3]) // 2
    draw.line((old_box[0], strike_y, old_box[2], strike_y), fill=VIOLET, width=3)
    draw.line((width // 2 - 38, y + 15, width // 2 + 38, y + 15), fill=GOLD, width=3)
    draw.polygon([(width // 2 + 38, y + 15), (width // 2 + 27, y + 8), (width // 2 + 27, y + 22)], fill=GOLD)
    draw.text((new_x, y), new_text, font=face, fill=PURPLE)


def make_frames():
    if not BACKGROUND.exists():
        raise FileNotFoundError(f"Missing artwork background: {BACKGROUND}")

    slides = [
        ("NOTECRAFT 0.0.9  |  PREVIEW", "Notes should outlive the app that wrote them.", "hero"),
        ("PLAIN FILES  |  PLAIN FOLDERS", "Your notes, in your folders", "plain"),
        ("TWO PAGE FORMATS", "Choose the format that fits", "formats"),
        ("A QUICK CHECKLIST", "Start with a notebook", "checklist"),
        ("WHY FOLDERS?", "Your pages stay understandable", "folders"),
        ("RICH NOTES", "Equations and diagrams belong on the page", "equation"),
        ("WRITE FREELY", "Use code, and cross things out", "code"),
        ("NOTECRAFT 0.0.9  |  PREVIEW", "Your notes. Your folders. Your choice.", "close"),
    ]

    frames = []
    for label, title, kind in slides:
        image, draw = frame_base(label, title)
        width, _ = image.size
        if kind == "hero":
            centered_paragraph(draw, "Pages are plain files in plain folders, so any sync tool works.", 267, get_font(27), INK, width, 900)
            centered(draw, "The 0.0.9 work is in progress:", 346, get_font(22), MUTED, width)
            draw_feature_pills(draw, width)
        elif kind == "plain":
            centered(draw, "A page and its pictures travel together.", 260, get_font(27), INK, width)
            centered(draw, "A synced folder gives you sync and backup with your own tools.", 318, get_font(23), MUTED, width)
            centered(draw, "No special notebook server is required.", 365, get_font(23), MUTED, width)
        elif kind == "formats":
            draw_format_table(draw, width)
        elif kind == "checklist":
            draw_checklist(draw, width)
        elif kind == "folders":
            draw_folder_cards(draw, width)
        elif kind == "equation":
            draw_equation_and_graph(draw, width)
        elif kind == "code":
            draw_code_and_strike(draw, width)
        elif kind == "close":
            centered(draw, "Create freely. Keep everything in ordinary folders.", 275, get_font(28), INK, width)
            centered(draw, "The koala is ready when you are.", 340, get_font(23), MUTED, width)
        frames.append(image.convert("RGB"))
    return frames


def main():
    frames = make_frames()
    frames[0].save(
        OUTPUT,
        format="GIF",
        save_all=True,
        append_images=frames[1:],
        duration=FRAME_DURATION_MS,
        loop=0,
        optimize=True,
        disposal=2,
    )
    print(f"Created {OUTPUT} ({len(frames)} frames, {FRAME_DURATION_MS} ms per frame)")


if __name__ == "__main__":
    main()
