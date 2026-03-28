import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SidebarNav } from "./SidebarNav";

describe("SidebarNav", () => {
  it("renders emoji-prefixed nav and helper copy button", () => {
    render(
      <SidebarNav
        activeTab="Projects"
        collapsed={false}
        statusMessage="Ready."
        onSelectTab={() => {}}
        onToggleCollapse={() => {}}
        onReindex={() => {}}
        onCopyHelperCommand={() => {}}
      />,
    );

    expect(screen.getByText("Projects")).toBeInTheDocument();
    expect(screen.getByText("📁")).toBeInTheDocument();
    expect(screen.getByText("cargo run")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument();
  });

  it("calls collapse toggle", () => {
    const onToggleCollapse = vi.fn();
    const view = render(
      <SidebarNav
        activeTab="Projects"
        collapsed={false}
        statusMessage="Ready."
        onSelectTab={() => {}}
        onToggleCollapse={onToggleCollapse}
        onReindex={() => {}}
        onCopyHelperCommand={() => {}}
      />,
    );

    fireEvent.click(view.getByRole("button", { name: "Collapse sidebar" }));
    expect(onToggleCollapse).toHaveBeenCalledTimes(1);
  });
});
