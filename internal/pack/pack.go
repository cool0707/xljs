// Package pack embeds a bundled script into a copy of the xljs executable
// and reads it back at startup.
//
// Layout appended to the executable (v2):
//
//	[zip archive][payload length uint64][magic "XLJSPAK2"]
//
// The zip holds manifest.json (script name and metadata), main.js (the
// bundled code) and assets/... (files listed by @html and @assets).
// Executables made by older versions (v1) are still read:
//
//	[name length uint32][name][code][payload length uint64][magic "XLJSPAK1"]
package pack

import (
	"archive/zip"
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"io/fs"
	"os"
	"path"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/meta"
)

var (
	magicV1 = []byte("XLJSPAK1")
	magicV2 = []byte("XLJSPAK2")
)

const trailerSize = 16

// Payload is an embedded script.
type Payload struct {
	Name string
	Code []byte
	// Meta is the script's metadata block (nil for v1 payloads).
	Meta *meta.Meta
	// Assets holds the files listed by Meta.AssetRoots. When writing, they
	// are read from this file system (the script folder); when reading,
	// it is the embedded copy. Nil when there are none.
	Assets fs.FS
}

type manifest struct {
	Version int        `json:"version"`
	Name    string     `json:"name"`
	Meta    *meta.Meta `json:"meta,omitempty"`
}

const (
	manifestName = "manifest.json"
	codeName     = "main.js"
	assetsDir    = "assets"
)

var errCorrupt = errors.New("corrupt payload")

// ReadSelf returns the payload of the running executable, or nil when the
// executable has none.
func ReadSelf() (*Payload, error) {
	exe, err := os.Executable()
	if err != nil {
		return nil, err
	}
	return Read(exe)
}

// Read returns the payload appended to the file at path, or nil.
func Read(file string) (*Payload, error) {
	f, err := os.Open(file)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	version, buf, err := readPayload(f)
	if err != nil || buf == nil {
		return nil, err
	}
	if version == 1 {
		nameLen := int(binary.LittleEndian.Uint32(buf[:4]))
		if 4+nameLen > len(buf) {
			return nil, errCorrupt
		}
		return &Payload{Name: string(buf[4 : 4+nameLen]), Code: buf[4+nameLen:]}, nil
	}
	zr, err := zip.NewReader(bytes.NewReader(buf), int64(len(buf)))
	if err != nil {
		return nil, errCorrupt
	}
	var m manifest
	data, err := fs.ReadFile(zr, manifestName)
	if err != nil || json.Unmarshal(data, &m) != nil {
		return nil, errCorrupt
	}
	code, err := fs.ReadFile(zr, codeName)
	if err != nil {
		return nil, errCorrupt
	}
	p := &Payload{Name: m.Name, Code: code, Meta: m.Meta}
	if _, err := fs.Stat(zr, assetsDir); err == nil {
		p.Assets, _ = fs.Sub(zr, assetsDir)
	}
	return p, nil
}

// readPayload returns the payload version and bytes, or a nil slice when the
// file has none.
func readPayload(f *os.File) (int, []byte, error) {
	size, version, err := payloadSize(f)
	if err != nil || version == 0 {
		return 0, nil, err
	}
	st, err := f.Stat()
	if err != nil {
		return 0, nil, err
	}
	buf := make([]byte, size)
	if _, err := f.ReadAt(buf, st.Size()-trailerSize-size); err != nil {
		return 0, nil, err
	}
	return version, buf, nil
}

// payloadSize reads the trailer. version is 0 when there is no payload.
func payloadSize(f *os.File) (size int64, version int, err error) {
	st, err := f.Stat()
	if err != nil {
		return 0, 0, err
	}
	if st.Size() < trailerSize {
		return 0, 0, nil
	}
	trailer := make([]byte, trailerSize)
	if _, err := f.ReadAt(trailer, st.Size()-trailerSize); err != nil {
		return 0, 0, err
	}
	switch {
	case bytes.Equal(trailer[8:], magicV1):
		version = 1
	case bytes.Equal(trailer[8:], magicV2):
		version = 2
	default:
		return 0, 0, nil
	}
	size = int64(binary.LittleEndian.Uint64(trailer[:8]))
	if size < 4 || size > st.Size()-trailerSize {
		return 0, 0, errCorrupt
	}
	return size, version, nil
}

// Write copies the running executable (without any existing payload) to
// out and appends the script.
func Write(out string, p *Payload) error {
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	return WriteFrom(exe, out, p)
}

// WriteFrom copies base (stripping an existing payload) to out and appends
// the script.
func WriteFrom(base, out string, p *Payload) error {
	if same(base, out) {
		return i18n.Err("pack.sameAsSelf", out)
	}
	archive, err := buildArchive(p)
	if err != nil {
		return err
	}
	src, err := os.Open(base)
	if err != nil {
		return err
	}
	defer src.Close()
	st, err := src.Stat()
	if err != nil {
		return err
	}
	size := st.Size()
	if old, version, err := payloadSize(src); err == nil && version != 0 {
		size -= old + trailerSize
	}
	dst, err := os.OpenFile(out, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
	if err != nil {
		return err
	}
	if _, err := io.CopyN(dst, src, size); err != nil {
		dst.Close()
		return err
	}
	archive = binary.LittleEndian.AppendUint64(archive, uint64(len(archive)))
	archive = append(archive, magicV2...)
	if _, err := dst.Write(archive); err != nil {
		dst.Close()
		return err
	}
	return dst.Close()
}

func buildArchive(p *Payload) ([]byte, error) {
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	add := func(name string, data []byte) error {
		w, err := zw.Create(name)
		if err == nil {
			_, err = w.Write(data)
		}
		return err
	}
	m, err := json.Marshal(manifest{Version: 2, Name: p.Name, Meta: p.Meta})
	if err != nil {
		return nil, err
	}
	if err := add(manifestName, m); err != nil {
		return nil, err
	}
	if err := add(codeName, p.Code); err != nil {
		return nil, err
	}
	if p.Assets != nil {
		seen := map[string]bool{}
		for _, root := range p.Meta.AssetRoots() {
			err := fs.WalkDir(p.Assets, root, func(name string, d fs.DirEntry, err error) error {
				if err != nil || d.IsDir() || seen[name] {
					return err
				}
				seen[name] = true
				data, err := fs.ReadFile(p.Assets, name)
				if err != nil {
					return err
				}
				return add(path.Join(assetsDir, name), data)
			})
			if err != nil {
				return nil, err
			}
		}
	}
	if err := zw.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func same(a, b string) bool {
	sa, err1 := os.Stat(a)
	sb, err2 := os.Stat(b)
	return err1 == nil && err2 == nil && os.SameFile(sa, sb)
}
