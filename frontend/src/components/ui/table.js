import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const Table = ({ class: cls, children, ...props }) =>
  html`<div data-slot="table-container" class="relative w-full overflow-x-auto">
    <table data-slot="table" class=${cn("w-full caption-bottom text-sm", cls)} ...${props}>${children}</table>
  </div>`;

export const TableHeader = ({ class: cls, children, ...props }) =>
  html`<thead data-slot="table-header" class=${cn("[&_tr]:border-b", cls)} ...${props}>${children}</thead>`;

export const TableBody = ({ class: cls, children, ...props }) =>
  html`<tbody data-slot="table-body" class=${cn("[&_tr:last-child]:border-0", cls)} ...${props}>${children}</tbody>`;

export const TableRow = ({ class: cls, children, ...props }) =>
  html`<tr data-slot="table-row" class=${cn("hover:bg-muted/50 data-[state=selected]:bg-muted border-b transition-colors", cls)} ...${props}>${children}</tr>`;

export const TableHead = ({ class: cls, children, ...props }) =>
  html`<th
    data-slot="table-head"
    class=${cn("text-foreground h-10 px-2 text-left align-middle font-medium whitespace-nowrap [&:has([role=checkbox])]:pr-0", cls)}
    ...${props}
  >${children}</th>`;

export const TableCell = ({ class: cls, children, ...props }) =>
  html`<td data-slot="table-cell" class=${cn("p-2 align-middle whitespace-nowrap [&:has([role=checkbox])]:pr-0", cls)} ...${props}>${children}</td>`;
