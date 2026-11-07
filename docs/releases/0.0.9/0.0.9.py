#!/usr/bin/env python3
"""Generate the v0.0.9 release-notes GIF and individual slide PNGs.

Usage: python docs/releases/0.0.9/0.0.9.py
Requires Pillow.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps


RELEASE_DIR = Path(__file__).resolve().parent
ART_DIR = RELEASE_DIR / "art"
BACKGROUND = ART_DIR / "0.0.9.jfif"
OUTPUT = RELEASE_DIR / "RELEASE_NOTES.gif"
FRAME_DIR = RELEASE_DIR / "frames"
AVATAR_DIR = RELEASE_DIR / "avatars"
FRAME_DURATION_MS = 2400
GIF_WIDTH = 960

PURPLE = (74, 39, 112, 255)
VIOLET = (126, 68, 181, 255)
MUTED = (102, 91, 112, 255)
INK = (55, 47, 64, 255)
WHITE = (255, 255, 255, 255)
RED = (210, 35, 45, 255)
BLUE = (25, 70, 155, 255)
GREEN = (20, 125, 65, 255)
YELLOW = (255, 205, 35, 255)


def get_font(size, bold=False):
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


def centered(draw, text, y, face, color, width):
    box = draw.textbbox((0, 0), text, font=face)
    x = (width - (box[2] - box[0])) // 2 - box[0]
    draw.text((x, y), text, font=face, fill=color)


def wrap_text(draw, text, face, max_width):
    lines = []
    for paragraph in text.splitlines():
        current = ""
        for word in paragraph.split():
            candidate = f"{current} {word}".strip()
            if current and draw.textlength(candidate, font=face) > max_width:
                lines.append(current)
                current = word
            else:
                current = candidate
        if current:
            lines.append(current)
    return lines


def flag(draw, x, y, w, h, locale):
    rect = (x, y, x + w, y + h)
    stripe = lambda i, count, color: draw.rectangle(
        (x, y + h * i // count, x + w, y + h * (i + 1) // count), fill=color
    )

    if locale in ("ar", "he"):
        draw.rounded_rectangle(rect, radius=5, fill=PURPLE, outline=(95, 85, 105, 255), width=1)
        glyph = "ض" if locale == "ar" else "א"
        face = get_font(31, bold=True)
        box = draw.textbbox((0, 0), glyph, font=face)
        glyph_x = x + (w - (box[2] - box[0])) // 2 - box[0]
        glyph_y = y + (h - (box[3] - box[1])) // 2 - box[1]
        draw.text((glyph_x, glyph_y), glyph, font=face, fill=WHITE)
    elif locale == "en":
        draw.rectangle(rect, fill=WHITE)
        for stripe_index in range(0, 13, 2):
            stripe_top = y + h * stripe_index / 13
            stripe_bottom = y + h * (stripe_index + 1) / 13
            draw.rectangle((x, stripe_top, x + w, stripe_bottom), fill=RED)

        canton_w = w * 0.4
        canton_h = h * 7 / 13
        draw.rectangle((x, y, x + canton_w, y + canton_h), fill=(60, 59, 110, 255))
        star_radius = min(w, h) * 0.025
        for row in range(9):
            stars_in_row = 6 if row % 2 == 0 else 5
            spacing = canton_w / 6
            offset = 0 if stars_in_row == 6 else spacing / 2
            for col in range(stars_in_row):
                center_x = x + spacing / 2 + offset + col * spacing
                center_y = y + canton_h * (row + 0.5) / 9
                draw.ellipse(
                    (
                        center_x - star_radius,
                        center_y - star_radius,
                        center_x + star_radius,
                        center_y + star_radius,
                    ),
                    fill=WHITE,
                )
    elif locale == "es":
        stripe(0, 4, RED)
        stripe(1, 4, YELLOW)
        stripe(2, 4, YELLOW)
        stripe(3, 4, RED)
    elif locale == "de":
        stripe(0, 3, (25, 25, 25, 255))
        stripe(1, 3, RED)
        stripe(2, 3, YELLOW)
    elif locale == "fr":
        draw.rectangle((x, y, x + w // 3, y + h), fill=BLUE)
        draw.rectangle((x + w // 3, y, x + 2 * w // 3, y + h), fill=WHITE)
        draw.rectangle((x + 2 * w // 3, y, x + w, y + h), fill=RED)
    elif locale == "hi":
        stripe(0, 3, (255, 145, 35, 255))
        stripe(1, 3, WHITE)
        stripe(2, 3, GREEN)
        draw.ellipse((x + w // 2 - 5, y + h // 2 - 5, x + w // 2 + 5, y + h // 2 + 5), outline=BLUE, width=2)
    elif locale == "ru":
        stripe(0, 3, WHITE)
        stripe(1, 3, BLUE)
        stripe(2, 3, RED)
    elif locale == "zh":
        draw.rectangle(rect, fill=RED)
        draw.polygon(star_points(x + 18, y + 16, 9, 4), fill=YELLOW)
        for sx, sy in ((33, 8), (39, 16), (39, 26), (32, 34)):
            draw.ellipse((x + sx, y + sy, x + sx + 4, y + sy + 4), fill=YELLOW)
    elif locale == "ja":
        draw.rectangle(rect, fill=WHITE)
        draw.ellipse((x + w // 2 - 11, y + h // 2 - 11, x + w // 2 + 11, y + h // 2 + 11), fill=RED)
    elif locale == "ko":
        draw.rectangle(rect, fill=WHITE)
        draw.pieslice((x + w // 2 - 12, y + h // 2 - 12, x + w // 2 + 12, y + h // 2 + 12), 0, 180, fill=RED)
        draw.pieslice((x + w // 2 - 12, y + h // 2 - 12, x + w // 2 + 12, y + h // 2 + 12), 180, 360, fill=BLUE)
        draw.ellipse((x + w // 2 - 12, y + h // 2 - 2, x + w // 2, y + h // 2 + 12), fill=RED)
        draw.ellipse((x + w // 2, y + h // 2 - 12, x + w // 2 + 12, y + h // 2 + 2), fill=BLUE)
    elif locale == "ar":
        draw.rectangle(rect, fill=(0, 105, 60, 255))
        draw.line((x + 16, y + h // 2, x + w - 15, y + h // 2), fill=WHITE, width=3)
        draw.line((x + w - 20, y + h // 2 + 5, x + w - 5, y + h // 2 + 5), fill=WHITE, width=2)
    elif locale == "fa":
        stripe(0, 3, GREEN)
        stripe(1, 3, WHITE)
        stripe(2, 3, RED)
    elif locale == "ur":
        draw.rectangle(rect, fill=GREEN)
        draw.rectangle((x, y, x + w // 5, y + h), fill=WHITE)
        draw.ellipse((x + w * 0.57, y + h * 0.23, x + w * 0.86, y + h * 0.77), fill=WHITE)
        draw.ellipse((x + w * 0.65, y + h * 0.17, x + w * 0.92, y + h * 0.71), fill=GREEN)
    elif locale == "he":
        draw.rectangle(rect, fill=WHITE)
        draw.rectangle((x, y + h // 5, x + w, y + h // 5 + 5), fill=BLUE)
        draw.rectangle((x, y + 4 * h // 5 - 5, x + w, y + 4 * h // 5), fill=BLUE)
        draw.polygon(star_points(x + w // 2, y + h // 2, 11, 6), outline=BLUE, width=2)

    draw.rectangle(rect, outline=(95, 85, 105, 255), width=1)


def star_points(cx, cy, outer_radius, inner_radius):
    import math

    return [
        (
            cx + (outer_radius if index % 2 == 0 else inner_radius) * math.cos(-math.pi / 2 + index * math.pi / 5),
            cy + (outer_radius if index % 2 == 0 else inner_radius) * math.sin(-math.pi / 2 + index * math.pi / 5),
        )
        for index in range(10)
    ]


def draw_locale_grid(draw, width):
    locales = [
        ("en", "English"),
        ("es", "Spanish"),
        ("de", "German"),
        ("fr", "French"),
        ("hi", "Hindi"),
        ("ru", "Russian"),
        ("zh", "Chinese"),
        ("ja", "Japanese"),
        ("ko", "Korean"),
        ("ar", "Arabic"),
        ("fa", "Persian"),
        ("ur", "Urdu"),
        ("he", "Hebrew"),
    ]
    cols = 4
    cell_w = 158
    start_x = (width - cols * cell_w) // 2
    for index, (locale, label) in enumerate(locales):
        row, col = divmod(index, cols)
        cell_x = start_x + col * cell_w
        flag_w, flag_h = 70, 42
        x = cell_x + (cell_w - flag_w) // 2
        y = 370 + row * 67
        flag(draw, x, y, flag_w, flag_h, locale)
        centered_at_x = cell_x + cell_w // 2
        label_font = get_font(17, bold=True)
        box = draw.textbbox((0, 0), label, font=label_font)
        draw.text((centered_at_x - (box[2] - box[0]) // 2, y + 45), label, font=label_font, fill=INK)


def draw_avatar(image, draw, avatar_path, width, top, size):
    avatar = Image.open(avatar_path).convert("RGB")
    avatar = ImageOps.fit(avatar, (size, size), centering=(0.5, 0.5))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, size - 1, size - 1), fill=255)
    left = (width - size) // 2
    image.paste(avatar, (left, top), mask)
    draw.ellipse((left, top, left + size - 1, top + size - 1), outline=VIOLET, width=4)


def frame(title, content, step, total, kind="text"):
    image = Image.open(BACKGROUND).convert("RGBA")
    width, height = image.size
    foreground_layer = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    header_draw = ImageDraw.Draw(foreground_layer, "RGBA")

    centered(header_draw, "NOTECRAFT 0.0.9", 75, get_font(56, bold=True), PURPLE, width)
    centered(header_draw, "Release notes", 145, get_font(17, bold=True), VIOLET, width)
    centered(header_draw, title, 220, get_font(43, bold=True), PURPLE, width)
    header_draw.line((width // 2 - 170, 285, width // 2 + 170, 285), fill=VIOLET, width=3)

    content_layer = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    content_draw = ImageDraw.Draw(content_layer, "RGBA")

    if kind == "flags":
        centered(content_draw, "13 interface locales", 309, get_font(27), MUTED, width)
        draw_locale_grid(content_draw, width)
    elif kind == "links":
        for index, (label, url) in enumerate(content):
            label_y = 310 if index == 0 else 505
            centered(content_draw, label, label_y, get_font(38, bold=True), PURPLE, width)
            url_lines = url if isinstance(url, list) else [url]
            for line_index, line in enumerate(url_lines):
                centered(content_draw, line, label_y + 54 + line_index * 42, get_font(30), MUTED, width)
    elif kind == "person":
        description, avatar_path = content
        avatar_size = 150
        avatar_y = 315
        draw_avatar(content_layer, content_draw, avatar_path, width, avatar_y, avatar_size)
        description_font = get_font(25)
        for line_index, line in enumerate(wrap_text(content_draw, description, description_font, width - 656)):
            centered(content_draw, line, 493 + line_index * 36, description_font, MUTED, width)
    else:
        lines = content if isinstance(content, list) else [content]
        face = get_font(31 if kind == "text" else 28)
        first_y = 326 if len(lines) > 1 else 345
        line_gap = 42
        for line_index, line in enumerate(lines):
            centered(content_draw, line, first_y + line_index * line_gap, face, MUTED, width)

    foreground_layer.alpha_composite(content_layer, dest=(0, 0))

    foreground_bounds = foreground_layer.getbbox()
    if foreground_bounds:
        foreground_center_x = (foreground_bounds[0] + foreground_bounds[2]) // 2
        foreground_center_y = (foreground_bounds[1] + foreground_bounds[3]) // 2
        target_center_x = width // 2
        target_center_y = (65 + height - 56) // 2
        image.alpha_composite(
            foreground_layer,
            dest=(target_center_x - foreground_center_x, target_center_y - foreground_center_y),
        )

    marker_y = height - 22
    marker_gap = min(32, 660 // max(total - 1, 1))
    marker_start = width // 2 - marker_gap * (total - 1) // 2
    draw = ImageDraw.Draw(image, "RGBA")
    for index in range(total):
        x = marker_start + index * marker_gap
        radius = 7 if index == step else 4
        color = VIOLET if index == step else (190, 165, 216, 255)
        draw.ellipse((x - radius, marker_y - radius, x + radius, marker_y + radius), fill=color)

    return image.convert("RGB")


def slides():
    feature_slides = [
        ("First Android release", ["NoteCraft comes to", "Android"], "01-android-release.png", "text"),
        ("Infinite canvas", ["Draw in", "open page space", "beyond text blocks"], "02-infinite-canvas.png", "text"),
        ("Languages", None, "03-languages.png", "flags"),
    ]

    link_slides = [
        ("Online links", [("Project profile", "https://notecraft.cloud/"), ("GitHub", ["https://github.com/", "AlienTechSoftware/notecraft"])], "04-links-project-github.png", "links"),
        ("Online links", [("Reddit", "https://www.reddit.com/r/notecraftai/"), ("Playground", "https://playground.daijro.dev/head/notecraft/")], "05-links-community.png", "links"),
        ("Online links", [("Discord", ["https://discord.com/", "channels/1359579021108842617/", "1559147620084088982"]), ("Contributors guide", "CONTRIBUTING.md")], "06-links-discord-guide.png", "links"),
    ]

    people = [
        ("@tanmaysinghx", "RTL layout, deadlock, and blank-box fixes."),
        ("@MPro87", "Added the app ID; reported Linux install issues."),
        ("@rastacalavera", "Built and verified macOS app, DMG, and CLI packages."),
        ("@MostafaXS1", "Created the ArtCraft Android project used by NoteCraft."),
        ("@AKRking", "Suggested improvements to pen input (#44)."),
        ("@Christian-Espinosa", "Reported cloud sign-in and import-color issues."),
        ("@AshOtis", "Reported image-insertion and theme issues."),
        ("@mgrenonville", "Requested a translated interface and French support."),
        ("@Virility", "Reported OneNote package and canvas issues.\nAlso reported JPEG XL issues."),
        ("@Mathew-D", "Reported Linux packaging and file picker issues.\nAlso reported deadlock and blank-box issues."),
        ("@sl-books", "Reported errors importing existing OneNote notebooks."),
        ("@BYIIHub", "Reported the missing macOS DMG."),
        ("@MSTSmusic", "Requested Android phone support."),
        ("@Devilmayycryyy", "Reported Linux pen-tablet input issues."),
    ]
    contributor_slides = [
        (
            name,
            (description, AVATAR_DIR / f"{name.lstrip('@').lower()}.jpg"),
            f"{index + 7:02d}-contributor-{name.lstrip('@').lower()}.png",
            "person",
        )
        for index, (name, description) in enumerate(people)
    ]
    return feature_slides + link_slides + contributor_slides


def make_frames():
    if not BACKGROUND.exists():
        raise FileNotFoundError(f"Missing artwork background: {BACKGROUND}")

    all_slides = slides()
    total = len(all_slides)
    return [
        (frame(title, content, index, total, kind), filename)
        for index, (title, content, filename, kind) in enumerate(all_slides)
    ]


def main():
    rendered = make_frames()
    FRAME_DIR.mkdir(parents=True, exist_ok=True)
    expected_names = {filename for _, filename in rendered}
    for old_frame in FRAME_DIR.glob("[0-9][0-9]-*.png"):
        if old_frame.name not in expected_names:
            old_frame.unlink()
    for image, filename in rendered:
        image.save(FRAME_DIR / filename, format="PNG", optimize=True)

    frames = [image for image, _ in rendered]
    gif_frames = [
        image.resize(
            (GIF_WIDTH, round(image.height * GIF_WIDTH / image.width)),
            Image.Resampling.LANCZOS,
        )
        for image in frames
    ]
    gif_frames[0].save(
        OUTPUT,
        format="GIF",
        save_all=True,
        append_images=gif_frames[1:],
        duration=FRAME_DURATION_MS,
        loop=0,
        optimize=True,
        disposal=2,
    )
    print(f"Created {OUTPUT} ({len(frames)} frames at {GIF_WIDTH}px wide, {FRAME_DURATION_MS} ms per frame)")


if __name__ == "__main__":
    main()
