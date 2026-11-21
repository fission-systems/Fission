// Source of testdata/go_hello_linux_amd64.elf, for the Go analyzer's
// version-detection test. From this directory, with go1.25.5:
//
//	GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -buildvcs=false \
//	    -ldflags='-s -w -buildid=' -o ../go_hello_linux_amd64.elf .
package main

func main() { println("fission go fixture") }
