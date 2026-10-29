#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
NoteCraft Release Notes GIF Generator - Simple Numbers, Clean Design

Usage:
    python release-notes-gif-generator.py <version>
    python release-notes-gif-generator.py 0.0.7
"""

import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

SCRIPT_DIR = Path(__file__).parent.absolute()

# Typography
FONT_SIZE_TITLE = 64
FONT_SIZE_SUBTITLE = 48
FONT_SIZE_BODY = 32
FONT_SIZE_NUMBER = 80
LINE_HEIGHT = 60
FRAME_DURATION = 3500

# Colors
COLOR_TITLE = (50, 50, 50)
COLOR_TEXT = (70, 70, 70)
COLOR_ACCENT = (100, 180, 255)
COLOR_ACCENT2 = (120, 220, 160)
COLOR_ACCENT3 = (255, 150, 100)
COLOR_NUMBER = (255, 255, 255)  # White numbers for contrast
COLORS = [COLOR_ACCENT, COLOR_ACCENT2, COLOR_ACCENT3]


def get_font(size, bold=False):
    """Load system font."""
    paths = [
        ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf"),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Helvetica.ttc"),
    ]

    for reg, bold_path in paths:
        path = bold_path if bold else reg
        try:
            if Path(path).exists():
                return ImageFont.truetype(path, size)
        except:
            pass

    return ImageFont.load_default()


def load_background(version):
    """Load background image."""
    bg_path = SCRIPT_DIR / version / f"{version}.jfif"
    if not bg_path.exists():
        print(f"Error: Background not found")
        sys.exit(1)

    img = Image.open(bg_path)
    if img.mode != "RGBA":
        img = img.convert("RGBA")

    print(f"✓ Background: {bg_path.name} ({img.size[0]}×{img.size[1]})")
    return img


def draw_decorative_line(draw, x_start, x_end, y, width=3, color=(100, 180, 255)):
    """Draw a decorative line."""
    draw.rectangle([x_start, y - width//2, x_end, y + width//2], fill=color)


def draw_number_circle(draw, x, y, number, circle_color, size=80):
    """Draw colored circle with large white number centered."""
    box_size = size + 20

    # Colored circle background
    draw.ellipse(
        [x - box_size//2, y - box_size//2, x + box_size//2, y + box_size//2],
        fill=circle_color
    )

    # White number inside - centered both horizontally and vertically
    try:
        number_font = get_font(FONT_SIZE_NUMBER, bold=True)
        text = str(number)

        # Measure text at origin to get dimensions
        bbox = draw.textbbox((0, 0), text, font=number_font)
        text_w = bbox[2] - bbox[0]
        text_h = bbox[3] - bbox[1]

        # Center by subtracting half width and height
        # textbbox returns (left, top, right, bottom)
        text_x = x - text_w // 2 - bbox[0]
        text_y = y - text_h // 2 - bbox[1]

        draw.text((text_x, text_y), text, font=number_font, fill=COLOR_NUMBER)
    except Exception as e:
        print(f"Warning: Could not draw number: {e}")


def create_frame(background, data, frame_num, version="0.0.7", color_idx=0):
    """Create a frame with number and version."""
    frame = background.copy()
    w, h = frame.size
    draw = ImageDraw.Draw(frame)

    # Light overlay at top
    overlay = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    overlay_draw = ImageDraw.Draw(overlay)
    for y in range(int(h * 0.4)):
        alpha = int(30 * (1 - (y / (h * 0.4))))
        overlay_draw.rectangle([(0, y), (w, y + 1)], fill=(0, 0, 0, alpha))
    frame = Image.alpha_composite(frame, overlay)
    draw = ImageDraw.Draw(frame)

    # Data
    title = data.get('title', '')
    subtitle = data.get('subtitle', '')
    body = data.get('body', [])
    number = data.get('number', frame_num)
    circle_color = data.get('circle_color', COLORS[color_idx % len(COLORS)])

    # Fonts
    title_font = get_font(FONT_SIZE_TITLE, bold=True)
    subtitle_font = get_font(FONT_SIZE_SUBTITLE)
    body_font = get_font(FONT_SIZE_BODY)

    y_offset = int(h * 0.08)

    # Number in circle
    draw_number_circle(draw, w // 2, y_offset + 50, number, circle_color, size=80)
    y_offset += 140

    # Title on all frames: NoteCraft v0.0.7 (large, black)
    frame_title = f"NoteCraft v{version}"
    bbox = draw.textbbox((0, 0), frame_title, font=title_font)
    text_w = bbox[2] - bbox[0]
    x = (w - text_w) // 2
    draw.text((x, y_offset), frame_title, font=title_font, fill=(0, 0, 0))  # Black
    y_offset += FONT_SIZE_TITLE + 20

    # Line
    draw_decorative_line(draw, int(w * 0.2), int(w * 0.8), y_offset, width=3, color=circle_color)
    y_offset += 30

    # Subtitle: feature title + feature subtitle
    feature_title = title  # e.g., "Ink & Stylus Support"
    if feature_title:
        bbox = draw.textbbox((0, 0), feature_title, font=subtitle_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2
        draw.text((x, y_offset), feature_title, font=subtitle_font, fill=circle_color)
        y_offset += FONT_SIZE_SUBTITLE + 15

    if subtitle:
        bbox = draw.textbbox((0, 0), subtitle, font=body_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2
        draw.text((x, y_offset), subtitle, font=body_font, fill=COLOR_TEXT)
        y_offset += FONT_SIZE_SUBTITLE + 15

    # Body
    for line in body:
        if not line.strip():
            y_offset += 15
            continue

        bbox = draw.textbbox((0, 0), line, font=body_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2

        draw.text((x, y_offset), line, font=body_font, fill=COLOR_TEXT)
        y_offset += LINE_HEIGHT

    return frame


def generate_frames(version, slides_data):
    """Generate frames."""
    frames_dir = SCRIPT_DIR / version / ".frames"
    frames_dir.mkdir(parents=True, exist_ok=True)

    for f in frames_dir.glob("*.png"):
        f.unlink()

    print(f"✓ Frames directory: {frames_dir.name}/")

    background = load_background(version)

    frame_paths = []
    print(f"Generating {len(slides_data)} frames:")

    for i, slide in enumerate(slides_data, 1):
        frame = create_frame(background, slide, i, version=version, color_idx=i)
        path = frames_dir / f"frame_{i:03d}.png"
        frame.save(path, "PNG")
        frame_paths.append(path)
        title = slide['title']
        print(f"  {i}. {title}")

    return frame_paths, frames_dir


def create_gif(frames_dir, output, duration=FRAME_DURATION):
    """Create GIF."""
    frames = sorted(frames_dir.glob("frame_*.png"))
    if not frames:
        print("Error: No frames")
        return False

    print(f"\nCreating GIF ({duration}ms per frame)...")

    try:
        pattern = str(frames_dir / "frame_*.png")
        for cmd in ["magick", "convert"]:
            try:
                import subprocess
                subprocess.run(
                    [cmd, "convert", "-delay", str(duration // 10), "-loop", "0",
                     pattern, str(output)],
                    check=True,
                    capture_output=True,
                    timeout=60
                )
                print(f"✓ ImageMagick: {output.name}")
                return True
            except (FileNotFoundError, subprocess.CalledProcessError):
                continue
    except:
        pass

    # Pillow fallback
    try:
        images = [Image.open(p) for p in frames]
        images[0].save(
            output,
            save_all=True,
            append_images=images[1:],
            duration=duration,
            loop=0,
            optimize=False
        )
        print(f"✓ Pillow: {output.name}")
        return True
    except Exception as e:
        print(f"Error: {e}")
        return False


def main():
    if len(sys.argv) < 2:
        print("Usage: python release-notes-gif-generator.py <version>")
        sys.exit(1)

    version = sys.argv[1]
    version_dir = SCRIPT_DIR / version

    if not version_dir.exists():
        print(f"Error: {version_dir} not found")
        sys.exit(1)

    print(f"NoteCraft v{version} Release Notes GIF\n")

    # Release highlights with numbers and colors
    slides = [
        {
            'number': 1,
            'circle_color': (100, 180, 255),
            'title': f'NoteCraft v{version}',
            'subtitle': 'Linux Support & Installation',
            'body': ['Cross-platform deployment', 'Windows, macOS, Linux ready'],
        },
        {
            'number': 2,
            'circle_color': (120, 220, 160),
            'title': 'Ink & Stylus Support',
            'subtitle': 'Windows Improvements',
            'body': ['Better pen input handling', 'Enhanced tablet experience'],
        },
        {
            'number': 3,
            'circle_color': (100, 180, 255),
            'title': 'RTL & CICD Fixes',
            'subtitle': 'Language & Pipeline',
            'body': ['Right-to-left text fixes', 'GitHub Actions improvements'],
        },
        {
            'number': 4,
            'circle_color': (120, 220, 160),
            'title': 'Local File Support',
            'subtitle': 'HTML & Markdown Open',
            'body': ['Open HTML files directly', 'Full Markdown support'],
        },
        {
            'number': 5,
            'circle_color': (255, 150, 100),
            'title': 'Vega + D3 Charts',
            'subtitle': 'Data Visualization',
            'body': ['Interactive visualizations', 'Professional charts'],
        },
        {
            'number': 6,
            'circle_color': (100, 180, 255),
            'title': 'Debug Logging',
            'subtitle': 'Improved Diagnostics',
            'body': ['Better troubleshooting', 'Enhanced diagnostics'],
        },
    ]

    frames, frames_dir = generate_frames(version, slides)
    if not frames:
        sys.exit(1)

    output_gif = version_dir / "RELEASE_NOTES.gif"
    if not create_gif(frames_dir, output_gif):
        sys.exit(1)

    size_mb = output_gif.stat().st_size / (1024 * 1024)
    print(f"\n✅ {output_gif.name} ({size_mb:.2f} MB, {len(frames)} frames)")
    print("\nFrames: 1=Quality, 2=RTL, 3=CI, 4=Testing, 5=Demo, 6=Ready")


if __name__ == "__main__":
    main()
