package vba

import (
	"encoding/binary"
	"errors"
)

// decompress expands a CompressedContainer ([MS-OVBA] 2.4.1): a signature
// byte 1 and chunks of up to 4096 decompressed bytes, each either stored
// as is or as token sequences (a flag byte, then 8 tokens: literal bytes
// and copy tokens referring back into the chunk).
func decompress(b []byte) ([]byte, error) {
	if len(b) == 0 || b[0] != 1 {
		return nil, errors.New("not a compressed container")
	}
	var out []byte
	pos := 1
	for pos+2 <= len(b) {
		h := binary.LittleEndian.Uint16(b[pos:])
		end := min(pos+int(h&0x0FFF)+3, len(b))
		pos += 2
		start := len(out)
		if h&0x8000 == 0 { // uncompressed chunk
			n := min(4096, len(b)-pos)
			out = append(out, b[pos:pos+n]...)
			pos += n
			continue
		}
		for pos < end {
			flags := b[pos]
			pos++
			for bit := 0; bit < 8 && pos < end; bit++ {
				if flags&(1<<bit) == 0 {
					out = append(out, b[pos])
					pos++
					continue
				}
				if pos+2 > end {
					return nil, errors.New("truncated copy token")
				}
				tok := int(binary.LittleEndian.Uint16(b[pos:]))
				pos += 2
				bits := copyTokenBits(len(out) - start)
				length := tok&(0xFFFF>>bits) + 3
				src := len(out) - (tok>>(16-bits) + 1)
				if src < start {
					return nil, errors.New("copy token out of range")
				}
				for i := 0; i < length; i++ {
					out = append(out, out[src+i])
				}
			}
		}
		pos = end
	}
	return out, nil
}

// copyTokenBits is the number of offset bits of a copy token at the given
// position in the decompressed chunk: ceil(log2(pos)), at least 4.
func copyTokenBits(pos int) int {
	bits := 4
	for 1<<bits < pos {
		bits++
	}
	return bits
}
