module github.com/envoyproxy/protoc-gen-validate/tests

go 1.12

require (
	golang.org/x/net v0.0.0-20210813160813-60bc85c4be6d
	golang.org/x/xerrors v0.0.0-20200804184101-5ec99f83aff1 // indirect
	google.golang.org/protobuf v1.33.0
)

replace github.com/envoyproxy/protoc-gen-validate => ../
