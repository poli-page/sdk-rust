// Minimal type definitions for the subset of the rustdoc JSON shape we read.
// rustdoc emits a much larger schema; this captures only the fields the
// extractor depends on. Pin the nightly date when expanding.

export interface RustdocCrate {
  readonly root: number;
  readonly crate_version: string;
  readonly format_version: number;
  readonly index: Record<string, Item>;
  readonly paths: Record<string, PathEntry>;
}

export interface PathEntry {
  readonly crate_id: number;
  readonly path: readonly string[];
  readonly kind: string;
}

export interface Item {
  readonly id: number;
  readonly crate_id: number;
  readonly name: string | null;
  readonly docs?: string | null;
  readonly inner: ItemInner;
  readonly visibility?: string | Record<string, unknown>;
  readonly attrs?: ReadonlyArray<unknown>;
  readonly span?: { readonly filename: string };
}

// `inner` is a tagged union — exactly one of these keys is present per item.
export interface ItemInner {
  readonly struct?: StructInner;
  readonly enum?: EnumInner;
  readonly function?: FunctionInner;
  readonly impl?: ImplInner;
  readonly module?: ModuleInner;
  readonly variant?: VariantInner;
  readonly struct_field?: TypeNode;
  readonly type_alias?: TypeAliasInner;
  readonly use?: UseInner;
}

export interface StructInner {
  readonly kind: { readonly plain?: { readonly fields: readonly number[] } } | string;
  readonly impls: readonly number[];
}

export interface EnumInner {
  readonly variants: readonly number[];
  readonly impls: readonly number[];
}

export interface FunctionInner {
  readonly sig: {
    readonly inputs: ReadonlyArray<[string, TypeNode]>;
    readonly output: TypeNode | null;
  };
  readonly header: {
    readonly is_async: boolean;
    readonly is_unsafe: boolean;
    readonly is_const: boolean;
    readonly abi?: string;
  };
}

export interface ImplInner {
  readonly trait: { readonly path: string; readonly id: number } | null;
  readonly for: TypeNode;
  readonly items: readonly number[];
}

export interface ModuleInner {
  readonly items: readonly number[];
}

export interface VariantInner {
  readonly kind: unknown;
}

export interface TypeAliasInner {
  readonly type: TypeNode;
}

export interface UseInner {
  readonly source: string;
  readonly name: string;
  readonly is_glob: boolean;
}

// Type nodes — rustdoc uses a tagged-union shape. We render to string and
// recognise just enough variants to format the surface types nicely.
export type TypeNode =
  | { readonly resolved_path: { readonly path: string; readonly id: number; readonly args?: TypeArgs | null } }
  | { readonly primitive: string }
  | { readonly generic: string }
  | { readonly borrowed_ref: { readonly lifetime: string | null; readonly is_mutable: boolean; readonly type: TypeNode } }
  | { readonly tuple: readonly TypeNode[] }
  | { readonly impl_trait: ReadonlyArray<unknown> }
  | { readonly dyn_trait: { readonly traits: ReadonlyArray<{ readonly trait: { readonly path: string } }> } }
  | { readonly slice: TypeNode }
  | { readonly array: { readonly type: TypeNode; readonly len: string } }
  | { readonly raw_pointer: { readonly is_mutable: boolean; readonly type: TypeNode } }
  | { readonly function_pointer: unknown }
  | { readonly qualified_path: unknown }
  | Record<string, unknown>;

export interface TypeArgs {
  readonly angle_bracketed?: {
    readonly args: ReadonlyArray<{ readonly type?: TypeNode; readonly lifetime?: string }>;
    readonly constraints: ReadonlyArray<unknown>;
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers used by the page builders.
// ─────────────────────────────────────────────────────────────────────────────

export function getItem(td: RustdocCrate, id: number | undefined | null): Item | undefined {
  if (id === undefined || id === null) return undefined;
  return td.index[String(id)];
}

export function findFirst(
  td: RustdocCrate,
  pred: (item: Item) => boolean,
): Item | undefined {
  for (const item of Object.values(td.index)) {
    if (item.crate_id === 0 && pred(item)) return item;
  }
  return undefined;
}

export function findStruct(td: RustdocCrate, name: string): Item | undefined {
  return findFirst(td, (it) => it.name === name && Boolean(it.inner.struct));
}

export function findEnum(td: RustdocCrate, name: string): Item | undefined {
  return findFirst(td, (it) => it.name === name && Boolean(it.inner.enum));
}

/**
 * Collect functions defined on inherent (non-trait) impl blocks of a struct.
 */
export function inherentMethods(td: RustdocCrate, struct: Item): Item[] {
  const impls = struct.inner.struct?.impls ?? struct.inner.enum?.impls ?? [];
  const out: Item[] = [];
  for (const implId of impls) {
    const impl = getItem(td, implId);
    if (!impl || !impl.inner.impl) continue;
    if (impl.inner.impl.trait !== null) continue; // skip trait impls
    for (const itemId of impl.inner.impl.items) {
      const item = getItem(td, itemId);
      if (!item) continue;
      if (item.inner.function) out.push(item);
    }
  }
  return out;
}

export function findMethodOn(
  td: RustdocCrate,
  structName: string,
  methodName: string,
): Item | undefined {
  const struct = findStruct(td, structName);
  if (!struct) return undefined;
  return inherentMethods(td, struct).find((m) => m.name === methodName);
}

export function findTopLevelFunction(td: RustdocCrate, name: string): Item | undefined {
  return findFirst(td, (it) => it.name === name && Boolean(it.inner.function));
}

// ─────────────────────────────────────────────────────────────────────────────
// Type rendering — produces an idiomatic Rust type string from a TypeNode.
// ─────────────────────────────────────────────────────────────────────────────

export function renderType(node: TypeNode | null | undefined): string {
  if (!node) return '()';
  if ('resolved_path' in node) {
    const { path, args } = node.resolved_path as { path: string; args?: TypeArgs | null };
    const baseName = path.split('::').at(-1) ?? path;
    const inner = renderTypeArgs(args);
    return inner ? `${baseName}${inner}` : baseName;
  }
  if ('primitive' in node) return (node.primitive as string);
  if ('generic' in node) return (node.generic as string);
  if ('borrowed_ref' in node) {
    const r = node.borrowed_ref as { is_mutable: boolean; type: TypeNode };
    return `&${r.is_mutable ? 'mut ' : ''}${renderType(r.type)}`;
  }
  if ('tuple' in node) {
    const items = (node.tuple as readonly TypeNode[]).map(renderType);
    return `(${items.join(', ')})`;
  }
  if ('impl_trait' in node) {
    const traits = (node.impl_trait as ReadonlyArray<unknown>)
      .map((b) => renderGenericBound(b))
      .filter(Boolean);
    return `impl ${traits.join(' + ')}`;
  }
  if ('dyn_trait' in node) {
    const traits = (node.dyn_trait as { traits: ReadonlyArray<{ trait: { path: string } }> }).traits
      .map((t) => t.trait.path.split('::').at(-1) ?? t.trait.path);
    return `dyn ${traits.join(' + ')}`;
  }
  if ('slice' in node) return `[${renderType(node.slice as TypeNode)}]`;
  if ('array' in node) {
    const a = node.array as { type: TypeNode; len: string };
    return `[${renderType(a.type)}; ${a.len}]`;
  }
  if ('raw_pointer' in node) {
    const p = node.raw_pointer as { is_mutable: boolean; type: TypeNode };
    return `*${p.is_mutable ? 'mut' : 'const'} ${renderType(p.type)}`;
  }
  return 'unknown';
}

function renderTypeArgs(args: TypeArgs | null | undefined): string {
  if (!args || !args.angle_bracketed) return '';
  const parts = args.angle_bracketed.args
    .map((a) => {
      if (a.type) return renderType(a.type);
      if (a.lifetime) return a.lifetime;
      return '_';
    })
    .filter(Boolean);
  if (parts.length === 0) return '';
  return `<${parts.join(', ')}>`;
}

function renderGenericBound(bound: unknown): string {
  if (typeof bound === 'object' && bound !== null) {
    const obj = bound as Record<string, unknown>;
    if ('trait_bound' in obj) {
      const tb = obj.trait_bound as { trait?: { path?: string } };
      const p = tb.trait?.path ?? '';
      return p.split('::').at(-1) ?? p;
    }
    if ('outlives' in obj) return String(obj.outlives);
  }
  return '';
}

// ─────────────────────────────────────────────────────────────────────────────
// Doc-comment helpers — pull a clean summary + first sentence out of the raw
// rustdoc `docs` field.
// ─────────────────────────────────────────────────────────────────────────────

export function docSummary(docs: string | null | undefined): string {
  if (!docs) return '';
  const lines = docs.split('\n');
  const out: string[] = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === '') break;          // first blank line ends the summary.
    if (trimmed.startsWith('# ')) break; // stop at the first section heading.
    out.push(trimmed);
  }
  return out.join(' ').trim();
}

export function firstSentence(text: string): string {
  if (!text) return '';
  const cleaned = text.replace(/\s+/g, ' ').trim();
  const m = cleaned.match(/^(.+?[.!?])(?:\s|$)/);
  return (m ? m[1]! : cleaned).slice(0, 150);
}
