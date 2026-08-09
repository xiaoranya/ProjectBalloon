pub(super) fn standard_output_matches(expected: &[u8], actual: &[u8]) -> bool {
    normalized_lines(expected) == normalized_lines(actual)
}

fn normalized_lines(content: &[u8]) -> Vec<Vec<u8>> {
    let mut normalized = Vec::with_capacity(content.len());
    let mut index = 0;
    while index < content.len() {
        if content[index] == b'\r' {
            normalized.push(b'\n');
            if content.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
        } else {
            normalized.push(content[index]);
        }
        index += 1;
    }
    let mut lines: Vec<Vec<u8>> = normalized
        .split(|byte| *byte == b'\n')
        .map(|line| {
            let end = line
                .iter()
                .rposition(|byte| !matches!(byte, b' ' | b'\t'))
                .map_or(0, |position| position + 1);
            line[..end].to_vec()
        })
        .collect();
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}
