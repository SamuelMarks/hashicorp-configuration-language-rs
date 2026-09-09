package main

import (
	"encoding/json"
	"os"

	"github.com/hashicorp/hcl/v2/gohcl"
	"github.com/hashicorp/hcl/v2/hclparse"
)

type Config struct {
	Name     string `hcl:"name" json:"name"`
	Count    int    `hcl:"count" json:"count"`
	IsActive bool   `hcl:"is_active" json:"is_active"`
}

func main() {
	parser := hclparse.NewParser()
	f, diags := parser.ParseHCLFile("test.hcl")
	if diags.HasErrors() {
		panic(diags)
	}
	var config Config
	diags = gohcl.DecodeBody(f.Body, nil, &config)
	if diags.HasErrors() {
		panic(diags)
	}

	bytes, _ := json.MarshalIndent(config, "", "  ")
    bytes = append(bytes, '\n')
	os.WriteFile("grounding.json", bytes, 0644)
}