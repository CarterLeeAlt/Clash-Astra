/** Family stack used by the Monaco code editors. */
export const editorFontFamily = `"JetBrains Mono", "Noto Sans SC", "twemoji mozilla"`;

/**
 * Monaco measures glyph widths once at editor creation. Load the bundled
 * JetBrains Mono face first so those measurements and the rendered code use
 * it instead of a fallback font. Failure is non-fatal: the editor still opens.
 */
export const loadEditorFont = () =>
  document.fonts.load(`14px "JetBrains Mono"`).catch(() => undefined);
