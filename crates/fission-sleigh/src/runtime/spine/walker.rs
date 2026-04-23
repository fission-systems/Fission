use super::RuntimeConstructNode;

#[derive(Debug, Clone)]
pub struct RuntimeParserWalker {
    construct_nodes: Vec<RuntimeConstructNode>,
}

impl RuntimeParserWalker {
    pub fn new(root_offset: usize, root_length: usize) -> Self {
        Self {
            construct_nodes: vec![RuntimeConstructNode {
                operand_index: None,
                parent_index: None,
                absolute_offset: root_offset,
                relative_length: root_length,
                handle_index: None,
            }],
        }
    }

    pub fn record_operand_node(
        &mut self,
        operand_index: usize,
        parent_index: usize,
        absolute_offset: usize,
        relative_length: usize,
        handle_index: usize,
    ) {
        self.construct_nodes.push(RuntimeConstructNode {
            operand_index: Some(operand_index),
            parent_index: Some(parent_index),
            absolute_offset,
            relative_length,
            handle_index: Some(handle_index),
        });
    }

    pub fn into_nodes(self) -> Vec<RuntimeConstructNode> {
        self.construct_nodes
    }
}
