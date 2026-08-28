export function layoutTextLines(text, maxWidth, measureText) {
  // Spec §5.5: wrap within the rect width. CJK text has no spaces, so
  // break per character for CJK runs while keeping Latin word wrapping.
  // Explicit empty lines are retained so the editor and exported annotation
  // agree on the text frame height.
  const lines = [];
  const availableWidth = Math.max(1, maxWidth);
  const paragraphs = text.split(/\r?\n/);
  paragraphs.forEach((paragraph) => {
    let line = "";
    const flush = () => {
      lines.push(line);
      line = "";
    };
    let index = 0;
    while (index < paragraph.length) {
      const ch = paragraph[index];
      const isCjk = /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uac00-\ud7af]/.test(ch);
      const candidate = line ? `${line}${ch}` : ch;
      if (measureText(candidate) > availableWidth && line !== "") {
        flush();
        continue;
      }
      if (isCjk) {
        line += ch;
      } else {
        const rest = paragraph.slice(index);
        const wordMatch = rest.match(/^\s*\S+/);
        const word = wordMatch ? wordMatch[0] : ch;
        const candidateWord = line
          ? `${line}${word}`
          : index === 0
            ? word
            : word.trimStart();
        if (measureText(candidateWord) > availableWidth && line) {
          flush();
          continue;
        }
        if (measureText(candidateWord) > availableWidth) {
          // A single URL or other unbroken Latin run must still stay inside
          // the annotation frame, matching the textarea's break-word style.
          for (const character of candidateWord) {
            if (line && measureText(`${line}${character}`) > availableWidth) {
              flush();
            }
            line += character;
          }
        } else {
          line = candidateWord;
        }
        index += word.length - 1;
      }
      index += 1;
    }
    flush();
  });
  return lines;
}

export function fitTextEditorFrame(options) {
  const {
    text,
    fontSize,
    x,
    y,
    maxWidth,
    boundsWidth,
    boundsHeight,
    measureText,
  } = options;
  const safeBoundsWidth = Math.max(1, boundsWidth);
  const safeBoundsHeight = Math.max(1, boundsHeight);
  const widthLimit = Math.max(1, Math.min(maxWidth, safeBoundsWidth));
  const longestExplicitLine = text
    .split(/\r?\n/)
    .reduce((longest, line) => Math.max(longest, measureText(line)), 0);
  const width = Math.min(
    Math.max(Math.min(120, widthLimit), Math.ceil(longestExplicitLine) + 16 + 2),
    widthLimit,
  );
  const visualLineCount = layoutTextLines(text, Math.max(1, width - 16), measureText).length;
  const lineHeight = fontSize * 1.25;
  const height = Math.min(
    Math.max(Math.min(34, safeBoundsHeight), Math.ceil(visualLineCount * lineHeight) + 10 + 2),
    safeBoundsHeight,
  );
  return {
    x: Math.min(Math.max(0, x), Math.max(0, safeBoundsWidth - width)),
    y: Math.min(Math.max(0, y), Math.max(0, safeBoundsHeight - height)),
    width,
    height,
  };
}
