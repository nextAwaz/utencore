    fn handle_indent(&mut self, current_indent: usize) -> Option<Token> {
        let last_indent = *self.indent_stack.last().unwrap();
        if current_indent > last_indent {
            self.indent_stack.push(current_indent);
            Some(self.make_token(TokenKind::Indent, ""))
        } else if current_indent < last_indent {
            while *self.indent_stack.last().unwrap() > current_indent {
                self.indent_stack.pop();
            }
            if *self.indent_stack.last().unwrap() != current_indent {
                self.indent_stack.push(current_indent);
                return Some(self.make_token(TokenKind::Dedent, ""));
            }
            Some(self.make_token(TokenKind::Dedent, ""))
        } else {
            None
        }
    }

    fn count_indent(&mut self) -> usize {
        let mut indent = 0;
        let saved_pos = self.pos;
        loop {
            match self.src.get(self.pos) {
                Some(' ') => { indent += 1; self.pos += 1; self.col += 1; }
                Some('\t') => { indent += 4; self.pos += 1; self.col += 1; }
                Some('#') | Some('\n') => {
                    self.pos = saved_pos;
                    return 0;
                }
                _ => break,
            }
        }
        indent
    }