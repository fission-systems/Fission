// Source of go_hello_linux_amd64.elf, for the Go analyzer's version-detection
// test. From this directory, with go1.25.5:
//
//	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -buildvcs=false \
//	    -ldflags='-s -w -buildid=' -o go_hello_linux_amd64.elf go_hello.go
//
// A single file rather than a module directory: tests walk testdata/ and read
// every entry as a binary.
package main

func main() { println("fission go fixture") }
