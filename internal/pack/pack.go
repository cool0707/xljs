// Package pack embeds a bundled script into a copy of the xljs executable
// and reads it back at startup.
//
// Layout appended to the executable:
//
//	[name length uint32][name][code][payload length uint64][magic "XLJSPAK1"]
package pack

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"
)

var magic = []byte("XLJSPAK1")

const trailerSize = 16

// Payload is an embedded script.
type Payload struct {
	Name string
	Code []byte
}

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
func Read(path string) (*Payload, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	st, err := f.Stat()
	if err != nil {
		return nil, err
	}
	if st.Size() < trailerSize {
		return nil, nil
	}
	trailer := make([]byte, trailerSize)
	if _, err := f.ReadAt(trailer, st.Size()-trailerSize); err != nil {
		return nil, err
	}
	if !bytes.Equal(trailer[8:], magic) {
		return nil, nil
	}
	size := int64(binary.LittleEndian.Uint64(trailer[:8]))
	if size < 4 || size > st.Size()-trailerSize {
		return nil, errors.New("corrupt payload")
	}
	buf := make([]byte, size)
	if _, err := f.ReadAt(buf, st.Size()-trailerSize-size); err != nil {
		return nil, err
	}
	nameLen := int(binary.LittleEndian.Uint32(buf[:4]))
	if 4+nameLen > len(buf) {
		return nil, errors.New("corrupt payload")
	}
	return &Payload{Name: string(buf[4 : 4+nameLen]), Code: buf[4+nameLen:]}, nil
}

// Write copies the running executable (without any existing payload) to
// out and appends the script.
func Write(out, name string, code []byte) error {
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	return WriteFrom(exe, out, name, code)
}

// WriteFrom copies base (stripping an existing payload) to out and appends
// the script.
func WriteFrom(base, out, name string, code []byte) error {
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
	if p, err := Read(base); err == nil && p != nil {
		size -= int64(4+len(p.Name)+len(p.Code)) + trailerSize
	}
	if same(base, out) {
		return fmt.Errorf("出力先が実行中の exe と同じです: %s", out)
	}
	dst, err := os.OpenFile(out, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
	if err != nil {
		return err
	}
	if _, err := io.CopyN(dst, src, size); err != nil {
		dst.Close()
		return err
	}
	payload := make([]byte, 0, 4+len(name)+len(code)+trailerSize)
	payload = binary.LittleEndian.AppendUint32(payload, uint32(len(name)))
	payload = append(payload, name...)
	payload = append(payload, code...)
	plen := uint64(len(payload))
	payload = binary.LittleEndian.AppendUint64(payload, plen)
	payload = append(payload, magic...)
	if _, err := dst.Write(payload); err != nil {
		dst.Close()
		return err
	}
	return dst.Close()
}

func same(a, b string) bool {
	sa, err1 := os.Stat(a)
	sb, err2 := os.Stat(b)
	return err1 == nil && err2 == nil && os.SameFile(sa, sb)
}
