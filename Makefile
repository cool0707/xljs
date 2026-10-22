VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
LDFLAGS := -s -w -X main.version=$(VERSION)
GO ?= go

.PHONY: all test vet windows windows-arm64 linux clean

all: test windows

test:
	$(GO) test ./...

vet:
	$(GO) vet ./...
	GOOS=windows $(GO) vet ./...

windows:
	GOOS=windows GOARCH=amd64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/xljs.exe ./cmd/xljs
	GOOS=windows GOARCH=amd64 $(GO) build -trimpath -ldflags "$(LDFLAGS) -H=windowsgui" -o dist/xljsw.exe ./cmd/xljs

windows-arm64:
	GOOS=windows GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/arm64/xljs.exe ./cmd/xljs
	GOOS=windows GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS) -H=windowsgui" -o dist/arm64/xljsw.exe ./cmd/xljs

linux:
	$(GO) build -trimpath -ldflags "$(LDFLAGS)" -o dist/xljs ./cmd/xljs

clean:
	rm -rf dist
