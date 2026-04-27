package types

import (
	"encoding/json"
	"encoding/xml"
	"errors"
	"strings"

	yaml "github.com/goccy/go-yaml"
)

type toolRequestEnvelope struct {
	Raw         string                 `json:"raw" yaml:"raw"`
	Name        string                 `json:"name" yaml:"name"`
	CallID      string                 `json:"callId" yaml:"callId"`
	CallIDSnake string                 `json:"call_id" yaml:"call_id"`
	Args        map[string]interface{} `json:"args" yaml:"args"`
	Arguments   map[string]interface{} `json:"arguments" yaml:"arguments"`
	Reason      string                 `json:"reason,omitempty" yaml:"reason,omitempty"`
}

type xmlToolRequest struct {
	Name       string         `xml:"name,attr"`
	CallID     string         `xml:"call_id,attr"`
	Parameters []xmlParameter  `xml:"parameter"`
}

type xmlParameter struct {
	Name  string `xml:"name,attr"`
	Value string `xml:",chardata"`
}

func (e toolRequestEnvelope) toToolRequest() *ToolRequest {
	req := &ToolRequest{
		Name:   e.Name,
		CallID: e.CallID,
		Reason: e.Reason,
	}
	if req.CallID == "" {
		req.CallID = e.CallIDSnake
	}
	if e.Args != nil {
		req.Args = e.Args
	} else {
		req.Args = e.Arguments
	}
	return req
}

func ParseToolRequestPayload(data []byte) (*ToolRequest, error) {
	trimmed := strings.TrimSpace(string(data))
	if trimmed == "" {
		return nil, errors.New("empty tool request payload")
	}

	if req, ok := parseJSONEnvelope([]byte(trimmed)); ok {
		return req, nil
	}
	if req, ok := parseYAMLEnvelope([]byte(trimmed)); ok {
		return req, nil
	}
	if req, ok := parseToolRequestText(trimmed); ok {
		return req, nil
	}

	return nil, errors.New("invalid tool request payload")
}

func parseJSONEnvelope(data []byte) (*ToolRequest, bool) {
	var env toolRequestEnvelope
	if err := json.Unmarshal(data, &env); err != nil {
		return nil, false
	}

	if env.Raw != "" {
		if req, ok := parseToolRequestText(env.Raw); ok {
			return req, true
		}
	}
	if env.Name != "" {
		return env.toToolRequest(), true
	}

	var wrapper struct {
		ToolCall toolRequestEnvelope `json:"tool_call"`
	}
	if err := json.Unmarshal(data, &wrapper); err == nil && wrapper.ToolCall.Name != "" {
		if wrapper.ToolCall.Raw != "" {
			if req, ok := parseToolRequestText(wrapper.ToolCall.Raw); ok {
				return req, true
			}
		}
		return wrapper.ToolCall.toToolRequest(), true
	}

	return nil, false
}

func parseYAMLEnvelope(data []byte) (*ToolRequest, bool) {
	var env toolRequestEnvelope
	if err := yaml.Unmarshal(data, &env); err != nil {
		return nil, false
	}

	if env.Raw != "" {
		if req, ok := parseToolRequestText(env.Raw); ok {
			return req, true
		}
	}
	if env.Name != "" {
		return env.toToolRequest(), true
	}

	var wrapper struct {
		ToolCall toolRequestEnvelope `yaml:"tool_call"`
	}
	if err := yaml.Unmarshal(data, &wrapper); err == nil && wrapper.ToolCall.Name != "" {
		if wrapper.ToolCall.Raw != "" {
			if req, ok := parseToolRequestText(wrapper.ToolCall.Raw); ok {
				return req, true
			}
		}
		return wrapper.ToolCall.toToolRequest(), true
	}

	return nil, false
}

func parseToolRequestText(text string) (*ToolRequest, bool) {
	trimmed := stripMarkdownFence(text)
	trimmed = strings.TrimSpace(trimmed)
	if trimmed == "" {
		return nil, false
	}

	if req, ok := parseXMLToolRequest(trimmed); ok {
		return req, true
	}
	if req, ok := parseYAMLToolRequest(trimmed); ok {
		return req, true
	}
	if req, ok := parseJSONToolRequest(trimmed); ok {
		return req, true
	}

	return nil, false
}

func parseXMLToolRequest(text string) (*ToolRequest, bool) {
	start := strings.Index(text, "<tool")
	if start == -1 {
		return nil, false
	}

	end := strings.LastIndex(text, "</tool>")
	closeTag := "</tool>"
	if end == -1 {
		end = strings.LastIndex(text, "</tool_call>")
		closeTag = "</tool_call>"
	}
	if end == -1 {
		return nil, false
	}

	segment := text[start : end+len(closeTag)]
	var tool xmlToolRequest
	if err := xml.Unmarshal([]byte(segment), &tool); err != nil || tool.Name == "" {
		return nil, false
	}

	args := make(map[string]interface{}, len(tool.Parameters))
	for _, p := range tool.Parameters {
		args[p.Name] = strings.TrimSpace(p.Value)
	}

	return &ToolRequest{
		Name:   tool.Name,
		CallID: tool.CallID,
		Args:   args,
	}, true
}

func parseYAMLToolRequest(text string) (*ToolRequest, bool) {
	var direct ToolRequest
	if err := yaml.Unmarshal([]byte(text), &direct); err == nil && direct.Name != "" {
		return &direct, true
	}

	var wrapper struct {
		ToolCall ToolRequest `yaml:"tool_call"`
	}
	if err := yaml.Unmarshal([]byte(text), &wrapper); err == nil && wrapper.ToolCall.Name != "" {
		return &wrapper.ToolCall, true
	}

	return nil, false
}

func parseJSONToolRequest(text string) (*ToolRequest, bool) {
	var direct ToolRequest
	if err := json.Unmarshal([]byte(text), &direct); err == nil && direct.Name != "" {
		return &direct, true
	}

	var wrapper struct {
		ToolCall ToolRequest `json:"tool_call"`
	}
	if err := json.Unmarshal([]byte(text), &wrapper); err == nil && wrapper.ToolCall.Name != "" {
		return &wrapper.ToolCall, true
	}

	return nil, false
}

func stripMarkdownFence(text string) string {
	trimmed := strings.TrimSpace(text)
	if !strings.HasPrefix(trimmed, "```") {
		return trimmed
	}

	firstNewline := strings.Index(trimmed, "\n")
	if firstNewline == -1 {
		return trimmed
	}

	lastFence := strings.LastIndex(trimmed, "```")
	if lastFence <= firstNewline {
		return trimmed
	}

	return strings.TrimSpace(trimmed[firstNewline+1 : lastFence])
}
