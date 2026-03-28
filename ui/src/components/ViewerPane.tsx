type ViewerPaneProps = {
  content?: string;
};

export function ViewerPane({ content }: ViewerPaneProps) {
  return <pre className="viewer-pre">{content ?? "Select a node."}</pre>;
}
