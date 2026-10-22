/**
 * 按 UTF-8 编码把字符串转成 base64。
 *
 * 直接调用 `btoa` 在遇到码点大于 0xFF 的字符（中文、emoji 等）时会抛
 * InvalidCharacterError；订阅与配置里的 SVG 图标常带中文注释，因此先转
 * UTF-8 字节再编码。分块拼接避免参数过多导致调用栈溢出。
 */
export const utf8ToBase64 = (input: string): string => {
  const bytes = new TextEncoder().encode(input);
  const chunkSize = 0x8000;

  let binary = "";
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }

  return btoa(binary);
};
