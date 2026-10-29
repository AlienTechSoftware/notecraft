#!/usr/bin/env python3
"""
NoteCraft Release Notes GIF Generator - Enhanced Edition

Generates vibrant, visually appealing animated GIFs with large typography,
icons, decorative elements, and professional layout.

Usage:
    python release-notes-gif-generator.py <version>
    python release-notes-gif-generator.py 0.0.7

Requirements:
    - Pillow (PIL): pip install Pillow
    - imagemagick (convert command): Optional, for higher quality output
"""

import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import subprocess

SCRIPT_DIR = Path(__file__).parent.absolute()

# Enhanced typography
FONT_SIZE_TITLE = 64
FONT_SIZE_SUBTITLE = 48
FONT_SIZE_BODY = 32
LINE_HEIGHT = 60
FRAME_DURATION = 3500

# Colors
COLOR_PRIMARY = (255, 255, 255)          # White text
COLOR_ACCENT = (100, 180, 255)           # Light blue
COLOR_ACCENT2 = (120, 220, 160)          # Teal
COLOR_ACCENT3 = (255, 150, 100)          # Coral
COLORS = [COLOR_ACCENT, COLOR_ACCENT2, COLOR_ACCENT3]


def get_font(size, bold=False):
    """Load system font with fallback."""
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
        print(f"Error: Background not found at {bg_path}")
        sys.exit(1)

    img = Image.open(bg_path)
    if img.mode != "RGBA":
        img = img.convert("RGBA")

    print(f"✓ Background: {bg_path.name} ({img.size[0]}×{img.size[1]})")
    return img


def draw_decorative_line(draw, x_start, x_end, y, width=3, color=(100, 180, 255)):
    """Draw a decorative line."""
    draw.rectangle([x_start, y - width//2, x_end, y + width//2], fill=color)


def draw_icon_box(draw, x, y, icon_text, color, size=60):
    """Draw a colored box with icon/text."""
    box_size = size + 20
    # Draw colored circle/box
    draw.ellipse(
        [x - box_size//2, y - box_size//2, x + box_size//2, y + box_size//2],
        fill=color
    )
    # Draw icon text
    icon_font = get_font(int(size * 0.6))
    bbox = draw.textbbox((0, 0), icon_text, font=icon_font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    draw.text(
        (x - text_w//2, y - text_h//2),
        icon_text,
        font=icon_font,
        fill=(255, 255, 255)
    )


def create_frame(background, data, frame_num, color_idx=0):
    """Create a visually rich frame."""
    frame = background.copy()
    w, h = frame.size
    draw = ImageDraw.Draw(frame)

    # Dark overlay for contrast
    overlay = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    overlay_draw = ImageDraw.Draw(overlay)
    for y in range(int(h * 0.5)):
        alpha = int(70 * (1 - (y / (h * 0.5))))
        overlay_draw.rectangle([(0, y), (w, y + 1)], fill=(0, 0, 0, alpha))
    frame = Image.alpha_composite(frame, overlay)
    draw = ImageDraw.Draw(frame)

    # Parse data: dict with 'title', 'subtitle', 'body', 'icon', 'color'
    title = data.get('title', '')
    subtitle = data.get('subtitle', '')
    body = data.get('body', [])
    icon = data.get('icon', '✨')
    color = data.get('color', COLORS[color_idx % len(COLORS)])

    # Fonts
    title_font = get_font(FONT_SIZE_TITLE, bold=True)
    subtitle_font = get_font(FONT_SIZE_SUBTITLE)
    body_font = get_font(FONT_SIZE_BODY)

    y_offset = int(h * 0.08)

    # Draw icon at top center
    if icon:
        draw_icon_box(draw, w // 2, y_offset + 50, icon, color, size=80)
        y_offset += 130

    # Draw title with accent color
    if title:
        bbox = draw.textbbox((0, 0), title, font=title_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2

        # Shadow
        draw.text((x + 2, y_offset + 2), title, font=title_font, fill=(0, 0, 0, 100))
        # Main text
        draw.text((x, y_offset), title, font=title_font, fill=color)
        y_offset += FONT_SIZE_TITLE + 20

    # Decorative line
    draw_decorative_line(draw, int(w * 0.2), int(w * 0.8), y_offset, width=3, color=color)
    y_offset += 30

    # Draw subtitle
    if subtitle:
        bbox = draw.textbbox((0, 0), subtitle, font=subtitle_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2

        draw.text((x + 2, y_offset + 2), subtitle, font=subtitle_font, fill=(0, 0, 0, 80))
        draw.text((x, y_offset), subtitle, font=subtitle_font, fill=COLOR_PRIMARY)
        y_offset += FONT_SIZE_SUBTITLE + 30

    # Draw body lines
    for line in body:
        if not line.strip():
            y_offset += 15
            continue

        bbox = draw.textbbox((0, 0), line, font=body_font)
        text_w = bbox[2] - bbox[0]
        x = (w - text_w) // 2

        draw.text((x + 1, y_offset + 1), line, font=body_font, fill=(0, 0, 0, 60))
        draw.text((x, y_offset), line, font=body_font, fill=COLOR_PRIMARY)
        y_offset += LINE_HEIGHT

    return frame


def generate_frames(version, slides_data):
    """Generate all frames."""
    frames_dir = SCRIPT_DIR / version / ".frames"
    frames_dir.mkdir(parents=True, exist_ok=True)

    for f in frames_dir.glob("*.png"):
        f.unlink()

    print(f"✓ Frames directory: {frames_dir.name}/")

    background = load_background(version)

    frame_paths = []
    print(f"Generating {len(slides_data)} frames:")

    for i, slide in enumerate(slides_data, 1):
        frame = create_frame(background, slide, i, color_idx=i)
        path = frames_dir / f"frame_{i:03d}.png"
        frame.save(path, "PNG")
        frame_paths.append(path)
        print(f"  {i}. {slide['title']}")

    return frame_paths, frames_dir


def create_gif(frames_dir, output, duration=FRAME_DURATION):
    """Create animated GIF."""
    frames = sorted(frames_dir.glob("frame_*.png"))
    if not frames:
        print("Error: No frames found")
        return False

    print(f"\nCreating GIF ({duration}ms per frame)...")

    # Try ImageMagick
    try:
        pattern = str(frames_dir / "frame_*.png")
        for cmd in ["magick", "convert"]:
            try:
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

    # Fallback to Pillow
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

    # Define slides with rich content
    slides = [
        {
            'icon': '1',
            'title': f'NoteCraft v{version}',
            'subtitle': 'Quality Release',
            'body': ['High-performance note taking', 'Cross-platform ready'],
        },
        {
            'icon': '2',
            'title': 'RTL Text Handling',
            'subtitle': 'Improved Caret',
            'body': ['Better cursor positioning', 'in right-to-left scripts'],
        },
        {
            'icon': '3',
            'title': 'GitHub Actions Fixed',
            'subtitle': 'CI Status Reports',
            'body': ['Proper branch protection integration', 'Zero-delay status reporting'],
        },
        {
            'icon': '4',
            'title': 'Installation Testing',
            'subtitle': 'Fresh Profile Verified',
            'body': ['Complete Windows verification', '13 language notebooks ready'],
        },
        {
            'icon': '5',
            'title': 'Demo Parity',
            'subtitle': 'Perfect Sync',
            'body': ['HTML and CMGFM identical', 'Pixel-perfect rendering'],
        },
        {
            'icon': '6',
            'title': 'Ready for Release',
            'subtitle': 'All Systems Go',
            'body': ['Full quality gate passed', 'Production ready'],
        },
    ]

    # Generate frames
    frames, frames_dir = generate_frames(version, slides)
    if not frames:
        sys.exit(1)

    # Create GIF
    output_gif = version_dir / "RELEASE_NOTES.gif"
    if not create_gif(frames_dir, output_gif):
        sys.exit(1)

    # Report
    size_mb = output_gif.stat().st_size / (1024 * 1024)
    print(f"\n✅ {output_gif.name} ({size_mb:.2f} MB, {len(frames)} frames)")
    print(f"\nMarkdown:\n![Release]" +
          f"(https://raw.githubusercontent.com/AlienTechSoftware/notecraft/main/docs/releases/{version}/RELEASE_NOTES.gif)")


if __name__ == "__main__":
    main()
