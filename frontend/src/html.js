// htm bound to Preact: tagged templates instead of JSX, no build step.
import { h } from "preact";
import htm from "htm";

export const html = htm.bind(h);
