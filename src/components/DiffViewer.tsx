type DiffViewerProps = {
  diff: string;
};

function diffLineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff --git")) {
    return "diff-line-meta";
  }
  if (line.startsWith("@@")) {
    return "diff-line-hunk";
  }
  if (line.startsWith("+") && !line.startsWith("+++")) {
    return "diff-line-add";
  }
  if (line.startsWith("-") && !line.startsWith("---")) {
    return "diff-line-del";
  }
  return "diff-line-context";
}

export function DiffViewer({ diff }: DiffViewerProps) {
  const lines = diff.split("\n");
  return (
    <pre className="diff-viewer">
      {lines.map((line, index) => (
        <div key={`${index}-${line.slice(0, 12)}`} className={`diff-line ${diffLineClass(line)}`}>
          {line || " "}
        </div>
      ))}
    </pre>
  );
}
