package ui

import (
	"bytes"
	"encoding/binary"
	"errors"
)

var pngMagic = []byte("\x89PNG\r\n\x1a\n")

// errBadIcon reports data that is neither an .ico file nor a PNG image.
var errBadIcon = errors.New("not an .ico or .png image")

// iconImage returns the image in an .ico file (or a PNG image) that best
// fits size pixels: the smallest one at least that large, else the largest.
// The result is a PNG or a DIB, as Windows' CreateIconFromResourceEx takes.
func iconImage(data []byte, size int) ([]byte, error) {
	if bytes.HasPrefix(data, pngMagic) {
		return data, nil
	}
	// ICONDIR: reserved 0, type 1, count; then 16-byte entries.
	if len(data) < 6 || binary.LittleEndian.Uint16(data) != 0 || binary.LittleEndian.Uint16(data[2:]) != 1 {
		return nil, errBadIcon
	}
	count := int(binary.LittleEndian.Uint16(data[4:]))
	if count == 0 || len(data) < 6+16*count {
		return nil, errBadIcon
	}
	var best []byte
	bestW, bestBPP := 0, 0
	better := func(w, bpp int) bool {
		switch {
		case best == nil:
			return true
		case (w >= size) != (bestW >= size):
			return w >= size // big enough wins
		case w != bestW:
			return (w >= size) == (w < bestW) // smallest big enough, or largest
		default:
			return bpp > bestBPP
		}
	}
	for i := 0; i < count; i++ {
		e := data[6+16*i:]
		w := int(e[0])
		if w == 0 {
			w = 256
		}
		bpp := int(binary.LittleEndian.Uint16(e[6:]))
		n := int(binary.LittleEndian.Uint32(e[8:]))
		off := int(binary.LittleEndian.Uint32(e[12:]))
		if n <= 0 || off < 0 || off+n > len(data) || off+n < off {
			continue
		}
		if better(w, bpp) {
			best, bestW, bestBPP = data[off:off+n], w, bpp
		}
	}
	if best == nil {
		return nil, errBadIcon
	}
	return best, nil
}

// CheckIcon reports whether data can be used as a window icon.
func CheckIcon(data []byte) error {
	_, err := iconImage(data, 32)
	return err
}
