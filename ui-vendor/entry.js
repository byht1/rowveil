// Re-exports the editor pieces used by static/app.js. Rebuild with `npm run build`.
export { EditorState, StateField, StateEffect, RangeSet, Compartment, Prec } from '@codemirror/state';
export {
  EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, dropCursor,
  rectangularSelection, crosshairCursor, highlightSpecialChars, gutter, GutterMarker, placeholder, Decoration, ViewPlugin,
} from '@codemirror/view';
export {
  defaultKeymap, history, historyKeymap, indentWithTab, toggleComment, selectLine, deleteLine,
  copyLineDown, copyLineUp, moveLineDown, moveLineUp, cursorMatchingBracket, insertNewlineAndIndent, addCursorAbove, addCursorBelow, selectAll,
} from '@codemirror/commands';
export {
  syntaxHighlighting, HighlightStyle, indentOnInput, bracketMatching, foldGutter, foldKeymap, indentUnit, foldAll, unfoldAll, foldable, foldEffect,
} from '@codemirror/language';
export {
  autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap, startCompletion, acceptCompletion,
} from '@codemirror/autocomplete';
export { searchKeymap, highlightSelectionMatches, selectNextOccurrence, selectSelectionMatches, openSearchPanel, searchPanelOpen, closeSearchPanel } from '@codemirror/search';
export { sql, PostgreSQL, MySQL, SQLite, StandardSQL } from '@codemirror/lang-sql';
export { tags } from '@lezer/highlight';
export { formatDialect, postgresql, mysql, mariadb, sqlite, bigquery, sql as standardSql } from 'sql-formatter';
export { json } from '@codemirror/lang-json';
