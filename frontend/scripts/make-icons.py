"""アイコン一式（favicon.ico、icon-192/512.png、apple-touch-icon.png）を、正方形の元の画像から作る。

- 元の画像が透明（RGBA）ならそのまま使う。不透明なら、外周からつながった背景色だけを透明にする
- 出力先の既定は public/（BrikRS に同梱する既定のアイコン）。
  サイトごとのアイコンを作るときは出力先を指定し、サーバーの BRANDING_DIR（既定 /data/branding）に置く

  pip install pillow
  python scripts/make-icons.py <元の画像> [出力先]
"""

import sys
from collections import deque
from pathlib import Path

from PIL import Image, ImageFilter

# 背景とみなす色の幅（元の背景色からの距離）。絵柄の濃い緑とは十分に離れている
TOLERANCE = 48
# iPhone のホーム画面は透明を黒で埋めるので、元の背景色（濃紺）で塗る
APPLE_BG = (5, 12, 30)


def transparent_background(src: Image.Image) -> Image.Image:
    if src.mode == "RGBA" and src.getpixel((0, 0))[3] == 0:
        return src.resize((1024, 1024), Image.LANCZOS)
    # 大きいまま塗りつぶすと遅いので、1024 に縮めてから処理する
    im = src.convert("RGB").resize((1024, 1024), Image.LANCZOS)
    w, h = im.size
    px = im.load()
    bg = px[0, 0]
    near = lambda c: sum((a - b) ** 2 for a, b in zip(c, bg)) <= TOLERANCE**2

    # 画像の外周からつながった背景色の領域を探す（縁取りで仕切られた外周の隙間も拾う）
    mask = Image.new("L", im.size, 255)
    mp = mask.load()
    border = [(x, y) for x in range(w) for y in (0, h - 1)] + [(x, y) for y in range(h) for x in (0, w - 1)]
    q = deque(border)
    seen = set(q)
    while q:
        x, y = q.popleft()
        if not near(px[x, y]):
            continue
        mp[x, y] = 0
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in seen:
                seen.add((nx, ny))
                q.append((nx, ny))

    # 背景との境目に残る暗いにじみを1px 削り、縁をなめらかにする
    mask = mask.filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(0.8))
    out = im.convert("RGBA")
    out.putalpha(mask)
    return out


def main() -> None:
    src = Image.open(sys.argv[1])
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "public")
    icon = transparent_background(src)
    out.mkdir(parents=True, exist_ok=True)

    def save(size: int, name: str, bg=None) -> None:
        im = icon.resize((size, size), Image.LANCZOS)
        if bg:
            base = Image.new("RGBA", im.size, bg + (255,))
            base.alpha_composite(im)
            im = base.convert("RGB")
        im.save(out / name, optimize=True)
        print(f"{out / name}  {size}x{size}  {(out / name).stat().st_size} bytes")

    save(512, "icon-512.png")  # PWA・共有カード（OGP）の既定の画像
    save(192, "icon-192.png")  # PWA・ナビバーのロゴ
    save(180, "apple-touch-icon.png", APPLE_BG)
    icon.resize((256, 256), Image.LANCZOS).save(out / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)])
    print(f"{out / 'favicon.ico'}  16/32/48  {(out / 'favicon.ico').stat().st_size} bytes")


if __name__ == "__main__":
    main()
