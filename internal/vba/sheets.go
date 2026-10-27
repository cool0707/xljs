package vba

import (
	"archive/zip"
	"encoding/xml"
	"io"
	"path"
	"strings"
)

// sheetCodeNames maps the code names of the sheets of an Office Open XML
// workbook (Sheet1, the names of their VBA modules) to the sheet names, read
// from <sheetPr codeName> of each sheet. Missing parts give an empty map.
func sheetCodeNames(zr *zip.Reader) map[string]string {
	files := map[string]*zip.File{}
	for _, f := range zr.File {
		files[strings.ToLower(f.Name)] = f
	}
	open := func(name string) io.ReadCloser {
		f := files[strings.ToLower(name)]
		if f == nil {
			return nil
		}
		r, err := f.Open()
		if err != nil {
			return nil
		}
		return r
	}
	out := map[string]string{}

	const wbPath = "xl/workbook.xml"
	var wb struct {
		Sheets []struct {
			Name string `xml:"name,attr"`
			RID  string `xml:"http://schemas.openxmlformats.org/officeDocument/2006/relationships id,attr"`
		} `xml:"sheets>sheet"`
	}
	var rels struct {
		Rels []struct {
			ID     string `xml:"Id,attr"`
			Target string `xml:"Target,attr"`
		} `xml:"Relationship"`
	}
	if !decodeXML(open(wbPath), &wb) || !decodeXML(open("xl/_rels/workbook.xml.rels"), &rels) {
		return out
	}
	targets := map[string]string{}
	for _, r := range rels.Rels {
		t := r.Target
		if strings.HasPrefix(t, "/") {
			t = t[1:]
		} else {
			t = path.Join(path.Dir(wbPath), t)
		}
		targets[r.ID] = t
	}
	for _, s := range wb.Sheets {
		if code := sheetCodeName(open(targets[s.RID])); code != "" {
			out[code] = s.Name
		}
	}
	return out
}

func decodeXML(r io.ReadCloser, v any) bool {
	if r == nil {
		return false
	}
	defer r.Close()
	return xml.NewDecoder(r).Decode(v) == nil
}

// sheetCodeName reads <sheetPr codeName> at the top of a worksheet (or
// chartsheet) part, without reading the cells.
func sheetCodeName(r io.ReadCloser) string {
	if r == nil {
		return ""
	}
	defer r.Close()
	d := xml.NewDecoder(r)
	depth := 0
	for {
		tok, err := d.Token()
		if err != nil {
			return ""
		}
		switch t := tok.(type) {
		case xml.StartElement:
			depth++
			if depth == 2 {
				if t.Name.Local != "sheetPr" {
					return ""
				}
				for _, a := range t.Attr {
					if a.Name.Local == "codeName" {
						return a.Value
					}
				}
				return ""
			}
		case xml.EndElement:
			depth--
		}
	}
}
