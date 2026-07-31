import assert from "node:assert/strict";
import test from "node:test";

import { trimBlankLines } from "../src/source-text.ts";

test("trims boundary blank lines without changing indentation or internal blanks", () => {
  assert.equal(
    trimBlankLines("\r\n \r\n  fn first() {}\r\n\r\n  fn second() {}\r\n \r\n"),
    "  fn first() {}\r\n\r\n  fn second() {}",
  );
  assert.equal(trimBlankLines("  fn indented() {}"), "  fn indented() {}");
  assert.equal(trimBlankLines(" \n\t\n"), "");
});
