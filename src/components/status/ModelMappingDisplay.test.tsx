import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ModelMappingDisplay } from "./ModelMappingDisplay";

describe("ModelMappingDisplay", () => {
  it("keeps stacked requested and upstream models for log tables", () => {
    const markup = renderToStaticMarkup(
      <ModelMappingDisplay requestedModel="gpt-5.2" resolvedModel="grok-4.6" />,
    );

    expect(markup).toContain(">gpt-5.2</div>");
    expect(markup).toContain(">grok-4.6</span>");
    expect(markup).toContain('title="gpt-5.2 → grok-4.6"');
  });

  it("renders an inline mapping on one line", () => {
    const markup = renderToStaticMarkup(
      <ModelMappingDisplay requestedModel="gpt-5.2" resolvedModel="grok-4.6" layout="inline" />,
    );

    expect(markup).toContain("gpt-5.2");
    expect(markup).toContain("→");
    expect(markup).toContain("grok-4.6");
    expect(markup).toContain('title="gpt-5.2 → grok-4.6"');
    expect(markup).not.toContain("pl-2");
  });

  it("shows only the requested model when nothing is mapped", () => {
    const markup = renderToStaticMarkup(
      <ModelMappingDisplay requestedModel="gpt-5.2" resolvedModel={null} layout="inline" />,
    );

    expect(markup).toContain("gpt-5.2");
    expect(markup).not.toContain("→");
  });
});
