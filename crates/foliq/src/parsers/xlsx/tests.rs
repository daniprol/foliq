//! Tests of the whole spreadsheet parser, on small XLSX files built here part
//! by part, so each test shows exactly what the file holds.

use std::io::Write;

use foliq_core::{Parser, SourceEntry};
use zip::write::SimpleFileOptions;

use super::XlsxParser;

/// Zips `(path, content)` parts into an XLSX file.
fn build_xlsx(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, content) in parts {
        writer.start_file(*path, SimpleFileOptions::default()).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn parse(bytes: &[u8]) -> String {
    let entry = SourceEntry {
        path: "book.xlsx".to_string(),
        uri: String::new(),
        version: String::new(),
        size: bytes.len() as u64,
        access: Vec::new(),
        metadata: Default::default(),
    };
    XlsxParser.parse(bytes, &entry).unwrap().text
}

const ROOT_RELATIONSHIPS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const WORKBOOK: &str = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Sales" sheetId="1" r:id="rId1"/><sheet name="Settings" sheetId="2" state="hidden" r:id="rId2"/></sheets>
<definedNames><definedName name="TaxRate">Settings!$B$2</definedName><definedName name="_xlnm.Print_Area" localSheetId="0">Sales!$A$1:$E$3</definedName></definedNames>
</workbook>"#;

const WORKBOOK_RELATIONSHIPS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const SHARED_STRINGS: &str = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<si><t>Date</t></si><si><t>Product</t></si><si><t>Price</t></si><si><t>Qty</t></si><si><t>Total</t></si>
<si><r><t>Widget </t></r><r><rPr><b/></rPr><t>A</t></r><rPh><t>ignored</t></rPh></si><si><t>Widget B</t></si>
<si><t>Discount</t></si><si><t>Tax rate</t></si>
</sst>"#;

/// Style 1 shows dates, style 2 percentages.
const STYLES: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts>
<cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="10"/></cellXfs>
</styleSheet>"#;

/// A table in A1:E3 with a date column, a link and a filled-down formula;
/// a second table in G1:G2, apart from the first.
const SALES_SHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c><c r="E1" t="s"><v>4</v></c><c r="G1" t="s"><v>7</v></c></row>
<row r="2"><c r="A2" s="1"><v>45296</v></c><c r="B2" t="s"><v>5</v></c><c r="C2"><v>12.5</v></c><c r="D2"><v>4</v></c><c r="E2"><f t="shared" ref="E2:E3" si="0">C2*D2</f><v>50</v></c><c r="G2" s="2"><v>0.1</v></c></row>
<row r="3"><c r="A3" s="1"><v>45297.5</v></c><c r="B3" t="s"><v>6</v></c><c r="C3"><v>0.1</v></c><c r="D3"><v>3</v></c><c r="E3"><f t="shared" si="0"/><v>0.30000000000000004</v></c></row>
</sheetData>
<hyperlinks><hyperlink ref="B2" r:id="rId1"/></hyperlinks>
</worksheet>"#;

const SALES_RELATIONSHIPS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/widget-a" TargetMode="External"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/>
</Relationships>"#;

const COMMENTS: &str = r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>Ana</author></authors>
<commentList><comment ref="C2" authorId="0"><text><r><rPr><b/></rPr><t>Ana:</t></r><r><t xml:space="preserve">
Check with supplier</t></r></text></comment></commentList></comments>"#;

const SETTINGS_SHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="2"><c r="A2" t="s"><v>8</v></c><c r="B2"><v>0.21</v></c></row>
</sheetData></worksheet>"#;

fn sales_workbook() -> Vec<u8> {
    build_xlsx(&[
        ("_rels/.rels", ROOT_RELATIONSHIPS),
        ("xl/workbook.xml", WORKBOOK),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELATIONSHIPS),
        ("xl/sharedStrings.xml", SHARED_STRINGS),
        ("xl/styles.xml", STYLES),
        ("xl/worksheets/sheet1.xml", SALES_SHEET),
        ("xl/worksheets/_rels/sheet1.xml.rels", SALES_RELATIONSHIPS),
        ("xl/comments1.xml", COMMENTS),
        ("xl/worksheets/sheet2.xml", SETTINGS_SHEET),
    ])
}

#[test]
fn values_are_written_as_people_read_them() {
    let text = parse(&sales_workbook());
    assert!(text.contains("| Date | Product | Price | Qty | Total |"), "{text}");
    // A date, a link, a rich-text string without its phonetic guide, a computed value.
    assert!(
        text.contains("| 2024-01-05 | [Widget A](https://example.com/widget-a) | 12.5 | 4 | 50 |"),
        "{text}"
    );
    // A date with a time, and 0.1 * 3 without the floating-point noise.
    assert!(
        text.contains("| 2024-01-06 12:00 | Widget B | 0.1 | 3 | 0.3 |"),
        "{text}"
    );
    assert!(!text.contains("ignored"));
}

#[test]
fn separate_blocks_are_separate_tables() {
    let text = parse(&sales_workbook());
    assert!(text.contains("Tables: A1:E3, G1:G2."), "{text}");
    assert!(text.contains("## G1:G2\n\n| Discount |\n| --- |\n| 10% |"), "{text}");
}

#[test]
fn formulas_comments_and_names_have_their_own_sections() {
    let text = parse(&sales_workbook());
    assert!(text.contains("## Formulas\n\n- E2:E3 (Total) = C2*D2"), "{text}");
    assert!(text.contains("## Notes\n\n- C2 (Ana): Check with supplier"), "{text}");
    assert!(text.contains("# Settings (hidden)"), "{text}");
    assert!(text.contains("# Names\n\n- TaxRate = Settings!$B$2"), "{text}");
    assert!(!text.contains("Print_Area"));
}

#[test]
fn big_tables_are_written_in_blocks_that_repeat_the_header() {
    let mut rows = String::from(
        r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row>"#,
    );
    for row in 2..=400 {
        rows.push_str(&format!(
            r#"<row r="{row}"><c r="A{row}" t="inlineStr"><is><t>Item number {row}</t></is></c><c r="B{row}"><v>{row}</v></c></row>"#
        ));
    }
    let sheet = format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Items" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let links = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let text = parse(&build_xlsx(&[
        ("_rels/.rels", ROOT_RELATIONSHIPS),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", links),
        ("xl/worksheets/sheet1.xml", &sheet),
    ]));
    assert!(text.contains("## A1:B400\n\n### Rows 2–"), "{text}");
    let blocks = text.matches("### Rows").count();
    assert!(blocks > 5, "{blocks} blocks");
    assert_eq!(text.matches("| Item | Amount |").count(), blocks);
    assert!(text.contains("| Item number 400 | 400 |"));
}

#[test]
fn pictures_placed_in_cells_take_their_alt_text() {
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>name</t></is></c><c r="B1" t="inlineStr"><is><t>photo</t></is></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>apple</t></is></c><c r="B2" t="e" vm="1"><v>#VALUE!</v></c></row>
<row r="3"><c r="A3" t="inlineStr"><is><t>pear</t></is></c><c r="B3" t="e" vm="2"><v>#VALUE!</v></c></row>
</sheetData></worksheet>"#;
    let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Fruit" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let links = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let metadata = r#"<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata"><metadataTypes count="1"><metadataType name="XLRICHVALUE"/></metadataTypes><futureMetadata name="XLRICHVALUE" count="2"><bk><extLst><ext><xlrd:rvb i="0"/></ext></extLst></bk><bk><extLst><ext><xlrd:rvb i="1"/></ext></extLst></bk></futureMetadata><valueMetadata count="2"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk></valueMetadata></metadata>"#;
    // The first picture has alt text; the second has none, so it borrows the words beside it.
    let rich_values = r#"<rvData xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata"><rv s="0"><v>0</v><v>5</v><v>A red apple</v></rv><rv s="1"><v>1</v><v>5</v></rv></rvData>"#;
    let structures = r#"<rvStructures xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata"><s t="_localImage"><k n="_rvRel:LocalImageIdentifier" t="i"/><k n="CalcOrigin" t="i"/><k n="Text" t="s"/></s><s t="_localImage"><k n="_rvRel:LocalImageIdentifier" t="i"/><k n="CalcOrigin" t="i"/></s></rvStructures>"#;
    let relations = r#"<richValueRels xmlns="http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><rel r:id="rId1"/><rel r:id="rId2"/></richValueRels>"#;
    let relation_links = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image2.png"/></Relationships>"#;
    let text = parse(&build_xlsx(&[
        ("_rels/.rels", ROOT_RELATIONSHIPS),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", links),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/metadata.xml", metadata),
        ("xl/richData/rdrichvalue.xml", rich_values),
        ("xl/richData/rdrichvaluestructure.xml", structures),
        ("xl/richData/richValueRel.xml", relations),
        ("xl/richData/_rels/richValueRel.xml.rels", relation_links),
    ]));
    assert!(
        text.contains("| apple | ![A red apple](xl/media/image1.png) |"),
        "{text}"
    );
    assert!(text.contains("| pear | ![pear](xl/media/image2.png) |"), "{text}");
    assert!(!text.contains("#VALUE!"));
}
