// Go extractors for the foliq benchmarks.
//
// `bench/tools.py` runs `foliq-bench-go <tool> <path>` once per file and reads
// the text from stdout. Go programs start in about a millisecond, so there is
// no need for a long-lived process as with Node or Java.
package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/ledongthuc/pdf"
	"github.com/xuri/excelize/v2"
)

// ledongthucPdf reads the text layer with ledongthuc/pdf, the most used pure-Go
// PDF reader, page by page in content-stream order.
func ledongthucPdf(path string) (string, error) {
	file, reader, err := pdf.Open(path)
	if err != nil {
		return "", err
	}
	defer file.Close()
	plain, err := reader.GetPlainText()
	if err != nil {
		return "", err
	}
	text, err := io.ReadAll(plain)
	return string(text), err
}

// excelizeXlsx writes every sheet under a heading, one row per line, cells
// separated by " | ".
func excelizeXlsx(path string) (string, error) {
	workbook, err := excelize.OpenFile(path)
	if err != nil {
		return "", err
	}
	defer workbook.Close()
	var text strings.Builder
	for _, sheet := range workbook.GetSheetList() {
		rows, err := workbook.GetRows(sheet)
		if err != nil {
			return "", err
		}
		fmt.Fprintf(&text, "# %s\n", sheet)
		for _, row := range rows {
			text.WriteString(strings.Join(row, " | ") + "\n")
		}
	}
	return text.String(), nil
}

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: foliq-bench-go <ledongthuc-pdf|excelize> <path>")
		os.Exit(2)
	}
	tools := map[string]func(string) (string, error){
		"ledongthuc-pdf": ledongthucPdf,
		"excelize":       excelizeXlsx,
	}
	text, err := tools[os.Args[1]](os.Args[2])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	os.Stdout.WriteString(text)
}
