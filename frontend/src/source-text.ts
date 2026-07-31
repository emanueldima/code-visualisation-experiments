export function trimBlankLines(source: string): string {
  if (source.trim() === "") return "";

  return source
    .replace(/^(?:[^\S\r\n]*(?:\r\n|\r|\n))+/, "")
    .replace(/(?:\r\n|\r|\n)[^\S\r\n]*(?:(?:\r\n|\r|\n)[^\S\r\n]*)*$/, "");
}
