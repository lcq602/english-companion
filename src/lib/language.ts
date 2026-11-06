// Change date: 2026-08-04 | Change: Add lightweight source language detection | Iteration: 1
export type Direction = "to-english" | "to-chinese";
export type Detection = Direction | "mixed" | "empty";

export function detectDirection(text: string): Detection {
  const trimmed = text.trim();
  if (!trimmed) return "empty";
  const cjk = (trimmed.match(/[\u3400-\u4dbf\u4e00-\u9fff]/g) ?? []).length;
  const latin = (trimmed.match(/[A-Za-z]/g) ?? []).length;
  if (cjk === 0 && latin > 0) return "to-chinese";
  if (latin === 0 && cjk > 0) return "to-english";
  if (cjk > 0 && latin > 0 && cjk / latin > 0.25 && cjk / latin < 4) return "mixed";
  if (cjk > latin * 0.35) return "to-english";
  if (latin > cjk * 0.35) return "to-chinese";
  return "mixed";
}
