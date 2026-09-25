/**
 * 模板正文编辑器：CodeMirror 6 + jinja2（legacy mode）语法高亮 + 等宽字体。
 *
 * 为什么带一个纯 `<textarea>` 兜底：CodeMirror 依赖真实布局测量，
 * 在无布局环境（jsdom 测试、SSR 预渲染）里会失败或产生噪音。
 * `codeMirrorSupported()` 在这些环境下返回 false，直接退到 textarea——
 * 组件对外的契约（受控 `value` + `onChange(string)`）完全一致，不影响页面逻辑。
 */
import { StreamLanguage } from "@codemirror/language";
import { jinja2 } from "@codemirror/legacy-modes/mode/jinja2";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { useEffect, useRef, useState } from "react";

export function codeMirrorSupported(): boolean {
  if (typeof window === "undefined" || typeof document === "undefined") return false;
  if (typeof document.createRange !== "function") return false;
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  return !/jsdom|happy-dom/i.test(ua);
}

/** 深色主题：与页面其余部分的 neutral 配色一致。 */
const darkTheme = EditorView.theme(
  {
    "&": {
      backgroundColor: "transparent",
      color: "#e5e5e5",
      fontSize: "13px",
    },
    ".cm-content": {
      fontFamily:
        'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace',
      padding: "8px 0",
    },
    ".cm-gutters": {
      backgroundColor: "transparent",
      color: "#525252",
      border: "none",
    },
    ".cm-activeLine": { backgroundColor: "rgba(255,255,255,0.03)" },
    ".cm-activeLineGutter": { backgroundColor: "transparent" },
    ".cm-selectionBackground, ::selection": { backgroundColor: "rgba(99,102,241,0.35)" },
    "&.cm-focused .cm-selectionBackground": { backgroundColor: "rgba(99,102,241,0.45)" },
    ".cm-cursor": { borderLeftColor: "#e5e5e5" },
  },
  { dark: true },
);

interface EditorProps {
  value: string;
  onChange: (next: string) => void;
}

function PlainEditor({ value, onChange }: EditorProps) {
  return (
    <textarea
      value={value}
      onChange={(event) => onChange(event.target.value)}
      spellCheck={false}
      className="min-h-72 w-full resize-y rounded-lg border border-neutral-800 bg-neutral-950 p-3 font-mono text-[13px] leading-relaxed text-neutral-100 outline-none focus:border-neutral-600"
    />
  );
}

function CodeEditor({ value, onChange }: EditorProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    const host = hostRef.current;
    if (host === null) return;
    const view = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          StreamLanguage.define(jinja2),
          EditorView.lineWrapping,
          darkTheme,
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChangeRef.current(update.state.doc.toString());
          }),
        ],
      }),
      parent: host,
    });
    viewRef.current = view;
    return () => {
      viewRef.current = null;
      view.destroy();
    };
    // 只在挂载时创建；外部 value 变化由下面的 effect 同步。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (view === null) return;
    if (view.state.doc.toString() !== value) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    }
  }, [value]);

  return (
    <div
      ref={hostRef}
      className="max-h-96 min-h-72 overflow-auto rounded-lg border border-neutral-800 bg-neutral-950 text-left"
    />
  );
}

export function TemplateEditor(props: EditorProps) {
  // 环境能力在挂载时确定一次即可（不会在运行中从有布局变成无布局）。
  const [supported] = useState(codeMirrorSupported);
  return supported ? <CodeEditor {...props} /> : <PlainEditor {...props} />;
}
