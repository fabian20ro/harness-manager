import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SidebarNav } from "./SidebarNav";

describe("SidebarNav", () => {
  it("renders emoji-prefixed nav without helper command UI", () => {
    render(
      <SidebarNav
        navigation={{
          activeTab: "Projects",
          onSelectTab: () => {},
        }}
        collapse={{
          collapsed: false,
          onToggleCollapse: () => {},
        }}
        reindex={{
          label: "Reindex all",
          onReindex: () => {},
        }}
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
        navigation={{
          activeTab: "Projects",
          onSelectTab: () => {},
        }}
        collapse={{
          collapsed: false,
          onToggleCollapse,
        }}
        reindex={{
          label: "Reindex all",
          onReindex: () => {},
        }}
      />,
    );

    fireEvent.click(view.getByRole("button", { name: "Collapse sidebar" }));
    expect(onToggleCollapse).toHaveBeenCalledTimes(1);
  });

  it("shows compact H brand when collapsed", () => {
    render(
      <SidebarNav
        navigation={{
          activeTab: "Projects",
          onSelectTab: () => {},
        }}
        collapse={{
          collapsed: true,
          onToggleCollapse: () => {},
        }}
        reindex={{
          label: "Reindex all",
          onReindex: () => {},
        }}
      />,
    );

    expect(screen.getByText("HI")).toBeInTheDocument();
    expect(screen.queryByText("Harness Inspector")).not.toBeInTheDocument();
  });
});
