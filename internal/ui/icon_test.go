package ui

import (
	"bytes"
	"encoding/binary"
	"testing"
)

// makeICO builds an .ico file with one fake image per size; each image's
// data is its size as text.
func makeICO(sizes ...int) []byte {
	var head, body bytes.Buffer
	binary.Write(&head, binary.LittleEndian, [3]uint16{0, 1, uint16(len(sizes))})
	off := 6 + 16*len(sizes)
	for _, s := range sizes {
		img := []byte{byte(s), byte(s >> 8)}
		w := byte(s)
		if s >= 256 {
			w = 0
		}
		head.Write([]byte{w, w, 0, 0})
		binary.Write(&head, binary.LittleEndian, [2]uint16{1, 32})
		binary.Write(&head, binary.LittleEndian, [2]uint32{uint32(len(img)), uint32(off)})
		body.Write(img)
		off += len(img)
	}
	return append(head.Bytes(), body.Bytes()...)
}

func TestIconImage(t *testing.T) {
	ico := makeICO(16, 256, 32, 48)
	for _, c := range []struct{ size, want int }{{16, 16}, {20, 32}, {32, 32}, {40, 48}, {64, 256}, {300, 256}} {
		img, err := iconImage(ico, c.size)
		if err != nil {
			t.Fatal(err)
		}
		if got := int(img[0]) | int(img[1])<<8; got != c.want {
			t.Errorf("size %d: got image %d, want %d", c.size, got, c.want)
		}
	}
	png := append(append([]byte{}, pngMagic...), 1, 2, 3)
	if img, err := iconImage(png, 32); err != nil || !bytes.Equal(img, png) {
		t.Errorf("png: %v", err)
	}
	broken := makeICO(32)
	binary.LittleEndian.PutUint32(broken[6+12:], 1000) // offset past the end
	for _, bad := range [][]byte{nil, []byte("GIF89a"), makeICO(), broken} {
		if CheckIcon(bad) == nil {
			t.Errorf("%q accepted", bad)
		}
	}
}
