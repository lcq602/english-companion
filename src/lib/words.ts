// 英文文本分词：把译文拆成可点击的单词 / 非单词片段，用于点词查释义。
export type WordToken = { text: string; isWord: boolean };

export function splitWords(text: string): WordToken[] {
  const tokens: WordToken[] = [];
  const regex = /[A-Za-z][A-Za-z'-]*/g;
  let last = 0;
  for (const match of text.matchAll(regex)) {
    const index = match.index ?? 0;
    if (index > last) tokens.push({ text: text.slice(last, index), isWord: false });
    tokens.push({ text: match[0], isWord: true });
    last = index + match[0].length;
  }
  if (last < text.length) tokens.push({ text: text.slice(last), isWord: false });
  return tokens;
}

/// 判断文本是否主要为英文（用于决定译文是否渲染为可点词模式）。
export function isMostlyEnglish(text: string): boolean {
  const latin = (text.match(/[A-Za-z]/g) ?? []).length;
  const cjk = (text.match(/[一-鿿]/g) ?? []).length;
  return latin > 0 && latin >= cjk;
}
