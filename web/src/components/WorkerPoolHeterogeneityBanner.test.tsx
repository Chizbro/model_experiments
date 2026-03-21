import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WorkerPoolHeterogeneityBanner } from "./WorkerPoolHeterogeneityBanner";

describe("WorkerPoolHeterogeneityBanner", () => {
  it("renders nothing when show is false", () => {
    const { container } = render(<WorkerPoolHeterogeneityBanner show={false} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders stable warning copy for heterogeneous pools", () => {
    render(<WorkerPoolHeterogeneityBanner show />);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Heterogeneous worker pool");
    expect(alert).toHaveTextContent("WSL");
    expect(alert).toHaveTextContent("Architecture §4c");
  });
});
