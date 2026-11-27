"""BrikRS に同梱する既定のアイコン（特定のサイトに依らない汎用のもの）の元画像を描く。

角丸の四角に、レンガを3段に積んだ絵柄（Brick = ブロックを積んで書く）。
出力した 1024px の PNG を make-icons.py に渡すと、サイズ別のアイコンができる。

  python scripts/default-icon.py /tmp/brikrs-icon.png
  python scripts/make-icons.py /tmp/brikrs-icon.png
"""

import sys

from PIL import Image, ImageDraw

S = 1024
BLUE = (35, 131, 226)  # Bootstrap テーマの $primary と同じ
DARK = (24, 98, 176)
WHITE = (255, 255, 255)

im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle((32, 32, S - 32, S - 32), radius=220, fill=BLUE)
# 下半分を少し濃くして奥行きを出す
d.rounded_rectangle((32, S // 2, S - 32, S - 32), radius=220, fill=DARK)
d.rectangle((32, S // 2, S - 32, S // 2 + 230), fill=DARK)

# レンガ：3段、段ごとに半分ずらす
left, right = 200, S - 200
brick_h, gap = 150, 36
top = (S - (brick_h * 3 + gap * 2)) // 2
width = right - left
for row in range(3):
    y = top + row * (brick_h + gap)
    if row % 2 == 0:
        xs = [(left, left + (width - gap) // 2), (left + (width + gap) // 2, right)]
    else:
        q = (width - gap * 2) // 4
        xs = [(left, left + q), (left + q + gap, right - q - gap), (right - q, right)]
    for x0, x1 in xs:
        d.rounded_rectangle((x0, y, x1, y + brick_h), radius=28, fill=WHITE)

im.save(sys.argv[1])
print(sys.argv[1])
