import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SidebarNav } from "./SidebarNav";

describe("SidebarNav", () => {
  it("renders emoji-prefixed nav without helper command UI", () => {
    render(
      <SidebarNav
        activeTab="Projects"
        collapsed={false}
        globalReindexLabel="Reindex all"
        onSelectTab={() => {}}
        onToggleCollapse={() => {}}
        onReindex={() => {}}
      />,
    );

    expect(screen.getByText("Projects")).toBeInTheDocument();
    expect(screen.getByText("📁")).toBeInTheDocument();
    expect(screen.queryByText("cargo run")).not.toBeInTheDocument();
    expect(screen.getByText("Harness Inspector")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Reindex all/ })).toBeInTheDocument();
  });

  it("calls collapse toggle", () => {
    const onToggleCollapse = vi.fn();
    const view = render(
      <SidebarNav
        activeTab="Projects"
        collapsed={false}
        globalReindexLabel="Reindex all"
        onSelectTab={() => {}}
        onToggleCollapse={onToggleCollapse}
        onReindex={() => {}}
      />,
    );

    fireEvent.click(view.getByRole("button", { name: "Collapse sidebar" }));
    expect(onToggleCollapse).toHaveBeenCalledTimes(1);
  });

  it("shows compact H brand when collapsed", () => {
    render(
      <SidebarNav
        activeTab="Projects"
        collapsed
        globalReindexLabel="Reindex all"
        onSelectTab={() => {}}
        onToggleCollapse={() => {}}
        onReindex={() => {}}
      />,
    );

    expect(screen.getByText("HI")).toBeInTheDocument();
    expect(screen.queryByText("Harness Inspector")).not.toBeInTheDocument();
  });
});
