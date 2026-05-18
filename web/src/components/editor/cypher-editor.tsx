import { useEffect, useRef } from 'react'
import { defaultKeymap } from '@codemirror/commands'
import {
  HighlightStyle,
  StreamLanguage,
  syntaxHighlighting,
} from '@codemirror/language'
import { EditorState, type Extension } from '@codemirror/state'
import { keymap, EditorView, drawSelection, highlightActiveLine } from '@codemirror/view'
import { tags } from '@lezer/highlight'

import { cn } from '@/lib/utils'

const cypherKeywords = new Set([
  'and',
  'as',
  'by',
  'create',
  'delete',
  'desc',
  'distinct',
  'false',
  'limit',
  'match',
  'merge',
  'not',
  'null',
  'optional',
  'or',
  'order',
  'return',
  'set',
  'skip',
  'true',
  'where',
  'with',
])

const cypherLanguage = StreamLanguage.define({
  startState() {
    return {
      inString: false,
      quote: '',
    }
  },
  token(stream, state) {
    if (state.inString) {
      let escaped = false

      while (!stream.eol()) {
        const char = stream.next()

        if (escaped) {
          escaped = false
          continue
        }

        if (char === '\\') {
          escaped = true
          continue
        }

        if (char === state.quote) {
          state.inString = false
          break
        }
      }

      return 'string'
    }

    if (stream.eatSpace()) {
      return null
    }

    if (stream.match('//')) {
      stream.skipToEnd()
      return 'comment'
    }

    const next = stream.peek()

    if (next === '"' || next === "'") {
      state.inString = true
      state.quote = stream.next() ?? '"'
      return 'string'
    }

    if (stream.match(/-?\d+(\.\d+)?/)) {
      return 'number'
    }

    if (stream.match(/[:()[\]{},.=<>*+\-/]/)) {
      return 'operator'
    }

    if (stream.match(/[A-Za-z_][A-Za-z0-9_]*/)) {
      const value = stream.current()

      if (cypherKeywords.has(value.toLowerCase())) {
        return 'keyword'
      }

      return 'variableName'
    }

    stream.next()
    return null
  },
})

const cypherHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: '#0f766e', fontWeight: '700' },
  { tag: tags.string, color: '#be123c' },
  { tag: tags.number, color: '#1d4ed8', fontWeight: '600' },
  { tag: tags.variableName, color: '#102a43' },
  { tag: tags.operator, color: '#9a3412' },
  { tag: tags.comment, color: '#64748b', fontStyle: 'italic' },
])

type CypherEditorProps = {
  value: string
  onChange: (value: string) => void
  onRun: (value: string) => void
  isRunning?: boolean
  className?: string
}

export function CypherEditor({
  value,
  onChange,
  onRun,
  isRunning = false,
  className,
}: CypherEditorProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const editorRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onRunRef = useRef(onRun)

  useEffect(() => {
    onChangeRef.current = onChange
    onRunRef.current = onRun
  }, [onChange, onRun])

  useEffect(() => {
    if (!containerRef.current) {
      return
    }

    const extensions: Extension[] = [
      EditorState.tabSize.of(2),
      keymap.of([
        ...defaultKeymap,
        {
          key: 'Ctrl-Enter',
          run: () => {
            onRunRef.current(view.state.doc.toString())
            return true
          },
        },
        {
          key: 'Mod-Enter',
          run: () => {
            onRunRef.current(view.state.doc.toString())
            return true
          },
        },
      ]),
      EditorView.lineWrapping,
      drawSelection(),
      highlightActiveLine(),
      cypherLanguage,
      syntaxHighlighting(cypherHighlightStyle),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString())
        }
      }),
      EditorView.theme({
        '&': {
          backgroundColor: 'transparent',
          color: '#102a43',
          fontSize: '14px',
          fontFamily: '"IBM Plex Mono", "SFMono-Regular", ui-monospace, monospace',
        },
        '.cm-content': {
          minHeight: '12rem',
          padding: '1rem',
          caretColor: '#0f766e',
        },
        '.cm-focused': {
          outline: 'none',
        },
        '.cm-scroller': {
          overflow: 'auto',
        },
        '.cm-activeLine': {
          backgroundColor: 'rgba(15, 118, 110, 0.08)',
        },
        '.cm-selectionBackground, .cm-content ::selection': {
          backgroundColor: 'rgba(20, 184, 166, 0.22)',
        },
        '.cm-cursor, .cm-dropCursor': {
          borderLeftColor: '#0f766e',
        },
      }),
    ]

    const state = EditorState.create({
      doc: value,
      extensions,
    })

    const view = new EditorView({
      state,
      parent: containerRef.current,
    })

    editorRef.current = view

    return () => {
      editorRef.current = null
      view.destroy()
    }
  }, [])

  useEffect(() => {
    const view = editorRef.current

    if (!view) {
      return
    }

    const currentValue = view.state.doc.toString()

    if (currentValue === value) {
      return
    }

    view.dispatch({
      changes: {
        from: 0,
        to: currentValue.length,
        insert: value,
      },
    })
  }, [value])

  return (
    <div
      data-testid="cypher-editor"
      className={cn(
        'overflow-hidden rounded-[1.35rem] border border-border/80 bg-[linear-gradient(180deg,_rgba(255,255,255,0.9),_rgba(240,248,245,0.96))] shadow-inner',
        className,
      )}
    >
      <div className="flex items-center justify-between border-b border-border/70 px-4 py-3 text-[11px] font-semibold uppercase tracking-[0.24em] text-muted-foreground">
        <span>Cypher editor</span>
        <span>{isRunning ? 'Running query…' : 'Ctrl+Enter to run'}</span>
      </div>
      <div ref={containerRef} />
    </div>
  )
}
