use std::collections::HashMap;

#[derive(Debug)]
#[allow(dead_code)]
pub struct Document {
    pub page_title: Option<String>,
    pub meta_tags: Vec<(String, String)>,
    pub head_blocks: Vec<String>,
    pub variables: HashMap<String, String>,
    pub defines: HashMap<String, Vec<Attribute>>,
    pub keyframes: Vec<(String, String)>,
    pub css_vars: Vec<(String, String)>,
    pub custom_css: Vec<String>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(Vec<TextSegment>),
    Raw(String),
}

#[derive(Debug, Clone)]
pub enum TextSegment {
    Plain(String),
    Inline(Element),
}

#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub attrs: Vec<Attribute>,
    pub argument: Option<String>,
    pub children: Vec<Node>,
    pub line_num: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    Row,
    Column,
    El,
    Text,
    Paragraph,
    Image,
    Link,
    Children,
    // Form elements
    Input,
    Button,
    Select,
    Textarea,
    Option,
    Label,
    // Slots
    Slot(String),
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub key: String,
    pub value: Option<String>,
}
