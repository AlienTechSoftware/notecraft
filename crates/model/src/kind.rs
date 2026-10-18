//! What a folder in a notebook is. All four are folders on disk; the kind is a role.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Notebook,
    SectionGroup,
    Section,
    Page,
}

impl NodeKind {
    pub fn id(self) -> &'static str {
        match self {
            NodeKind::Notebook => "notebook",
            NodeKind::SectionGroup => "section-group",
            NodeKind::Section => "section",
            NodeKind::Page => "page",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Notebook => "Notebook",
            NodeKind::SectionGroup => "Section group",
            NodeKind::Section => "Section",
            NodeKind::Page => "Page",
        }
    }

    pub fn parse(text: &str) -> Option<NodeKind> {
        match text.trim() {
            "notebook" => Some(NodeKind::Notebook),
            "section-group" => Some(NodeKind::SectionGroup),
            "section" => Some(NodeKind::Section),
            "page" => Some(NodeKind::Page),
            _ => None,
        }
    }

    /// Which kinds may be created directly inside this one. A page can hold
    /// pages (sub-pages) to any depth; OneNote stops at three levels.
    pub fn may_contain(self) -> &'static [NodeKind] {
        match self {
            NodeKind::Notebook | NodeKind::SectionGroup => &[NodeKind::SectionGroup, NodeKind::Section, NodeKind::Page],
            NodeKind::Section => &[NodeKind::Page],
            NodeKind::Page => &[NodeKind::Page],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for k in [NodeKind::Notebook, NodeKind::SectionGroup, NodeKind::Section, NodeKind::Page] {
            assert_eq!(NodeKind::parse(k.id()), Some(k));
        }
        assert_eq!(NodeKind::parse("folder"), None);
    }

    #[test]
    fn pages_nest_without_a_limit() {
        assert!(NodeKind::Page.may_contain().contains(&NodeKind::Page));
        assert!(!NodeKind::Section.may_contain().contains(&NodeKind::Section));
    }
}
