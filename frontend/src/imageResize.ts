/*
 * アップロード前の画像の縮小（サイト設定の image_max_px）。
 * 長い辺が上限を超える JPEG / PNG / WebP を、ブラウザの canvas で縮めてから送る（大きな画角の写真の転送を減らす）。
 * GIF は動きが消えるので縮めない。縮めた方がファイルが大きくなったとき・形式を書き出せないとき・
 * 読み込めないときは元のファイルのまま送る。描き直すので Exif（撮影位置など）は残らない
 */

const RESIZABLE = ['image/jpeg', 'image/png', 'image/webp']
const QUALITY = 0.9

let maxPx = 0

/** サイト設定を読んだとき・変えたときに呼ぶ（App.tsx） */
export function configureImageResize(value: string | number) {
  const n = Number(value)
  maxPx = Number.isFinite(n) && n > 0 ? Math.floor(n) : 0
}

export async function shrinkImage(file: File): Promise<File> {
  if (!maxPx || !RESIZABLE.includes(file.type)) return file
  let bitmap: ImageBitmap
  try {
    // 縦向きで撮った写真も、向きを直した状態で読む
    bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' })
  } catch {
    return file
  }
  try {
    const scale = maxPx / Math.max(bitmap.width, bitmap.height)
    if (scale >= 1) return file
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(bitmap.width * scale))
    canvas.height = Math.max(1, Math.round(bitmap.height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) return file
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, file.type, QUALITY))
    // 書き出せない形式だと PNG になる（Safari の WebP など）。そのときは元のまま
    if (!blob || blob.type !== file.type || blob.size >= file.size) return file
    return new File([blob], file.name, { type: file.type, lastModified: file.lastModified })
  } finally {
    bitmap.close()
  }
}
