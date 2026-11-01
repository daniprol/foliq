// JavaScript extractors for the foliq benchmarks, run as one long-lived process.
//
// `bench/tools.py` starts `node extract.mjs` once per worker and writes one
// JSON request per line: {"tool": "pdfjs", "path": "/a/b.pdf"}. Each answer is
// one JSON line: {"text": "..."} or {"error": "..."}. Keeping the process alive
// means Node's start-up time is not counted for every file.

import { readFile } from "node:fs/promises";
import { createInterface } from "node:readline";

import mammoth from "mammoth";
import officeParser from "officeparser";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import XLSX from "xlsx";

// Mozilla pdf.js, the engine of Firefox's PDF viewer: text items of each page,
// with a line break where pdf.js marks the end of a line. Same pdf.js version
// as officeparser uses (see package.json), since both share one worker.
async function pdfjs(path) {
  const data = new Uint8Array(await readFile(path));
  const loading = getDocument({ data, verbosity: 0, isEvalSupported: false });
  const document = await loading.promise;
  const pages = [];
  try {
    for (let number = 1; number <= document.numPages; number++) {
      const page = await document.getPage(number);
      const content = await page.getTextContent();
      pages.push(content.items.map((item) => item.str + (item.hasEOL ? "\n" : "")).join(""));
      page.cleanup();
    }
  } finally {
    await loading.destroy();
  }
  return pages.join("\n");
}

// officeparser: one library for PDF and Office files, asked for Markdown.
// The metadata block it puts first (`---` ... `---`) is not document text.
async function officeparser(path) {
  const document = await officeParser.parseOffice(path);
  const markdown = (await document.to("md")).value;
  return markdown.replace(/^---\n[\s\S]*?\n---\n/, "");
}

// mammoth: DOCX to Markdown, the popular JavaScript choice. Images are left
// out (by default they come back as long base64 strings).
async function mammothDocx(path) {
  const noImages = mammoth.images.imgElement(() => ({ src: "" }));
  return (await mammoth.convertToMarkdown({ path }, { convertImage: noImages })).value;
}

// SheetJS: every sheet as CSV under a heading with its name.
async function sheetjs(path) {
  const workbook = XLSX.read(await readFile(path));
  return workbook.SheetNames.map((name) => `# ${name}\n${XLSX.utils.sheet_to_csv(workbook.Sheets[name])}`).join("\n");
}

// Libraries that log must not mix with the answers on stdout.
console.log = console.error;
console.warn = console.error;

const TOOLS = { pdfjs, officeparser, mammoth: mammothDocx, sheetjs };

const lines = createInterface({ input: process.stdin });
for await (const line of lines) {
  const request = JSON.parse(line);
  let answer;
  try {
    answer = { text: await TOOLS[request.tool](request.path) };
  } catch (error) {
    answer = { error: String(error?.message ?? error).slice(0, 300) };
  }
  process.stdout.write(JSON.stringify(answer) + "\n");
}
