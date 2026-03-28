import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ViewerPane } from "./ViewerPane";

describe("ViewerPane", () => {
  it("renders wrapped viewer content in a dedicated pre element", () => {
    render(<ViewerPane content={"alpha ".repeat(50)} />);
    const viewer = screen.getByText(/alpha alpha/);
    expect(viewer.tagName).toBe("PRE");
    expect(viewer).toHaveClass("viewer-pre");
  });
});
