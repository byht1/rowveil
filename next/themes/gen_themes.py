"""Builds next/src/themes.json (gpui-component themes) from the stable UI's CSS themes.

Run from the repository root:  python next/themes/gen_themes.py
"""
import json
import re

CSS = open('static/style.css', encoding='utf-8').read()
THEMES = [
    ('rowveil-light', 'Rowveil Light'),
    ('rowveil', 'Rowveil Dark'),
    ('dark', 'Graphite'),
    ('light', 'Paper'),
    ('one-dark-pro', 'One Dark Pro'),
    ('dracula', 'Dracula'),
    ('nord', 'Nord'),
    ('github-light', 'GitHub Light'),
]


def block(theme_id):
    m = re.search(r':root\[data-theme="' + re.escape(theme_id) + r'"\]\s*\{(.*?)\n\}', CSS, re.S)
    if not m:
        m = re.search(r':root, :root\[data-theme="' + re.escape(theme_id) + r'"\]\s*\{(.*?)\n\}', CSS, re.S)
    body = m.group(1)
    v = {}
    v.update(dict(re.findall(r'--([\w-]+):\s*([^;]+);', body)))
    v['scheme'] = 'dark' if 'color-scheme: dark' in body else 'light'
    return v


def color(s):
    s = s.strip()
    m = re.match(r'rgba?\(([\d.\s]+),([\d.\s]+),([\d.\s]+)(?:,([\d.\s]+))?\)', s)
    if m:
        r, g, b = (int(float(x)) for x in m.groups()[:3])
        a = float(m.group(4)) if m.group(4) else 1.0
        return '#%02x%02x%02x%02x' % (r, g, b, round(a * 255))
    return s


def config(theme_id, name):
    v = {k: color(x) for k, x in block(theme_id).items()}
    c = lambda k: v[k]
    colors = {
        'background': c('bg'), 'foreground': c('text'), 'border': c('border'), 'caret': c('text'),
        'input.border': c('border-strong'), 'ring': c('sel'),
        'muted.background': c('panel-2'), 'muted.foreground': c('muted'),
        'secondary.background': c('elev'), 'secondary.foreground': c('text'), 'secondary.hover.background': c('hover'), 'secondary.active.background': c('border'),
        'accent.background': c('hover'), 'accent.foreground': c('text'),
        'primary.background': c('accent'), 'primary.foreground': c('accent-text'), 'primary.hover.background': c('accent'), 'primary.active.background': c('accent'),
        'danger.background': c('danger'), 'danger.foreground': '#ffffff', 'danger.hover.background': c('danger'),
        'success.background': c('true'), 'success.foreground': '#0b0b0b',
        'warning.background': '#facc15', 'warning.foreground': '#0b0b0b',
        'info.background': c('sel'), 'info.foreground': '#ffffff',
        'sidebar.background': c('panel'), 'sidebar.foreground': c('text'), 'sidebar.border': c('border'),
        'sidebar.accent.background': c('hover'), 'sidebar.accent.foreground': c('text'),
        'sidebar.primary.background': c('accent'), 'sidebar.primary.foreground': c('accent-text'),
        'popover.background': c('panel'), 'popover.foreground': c('text'),
        'list.background': c('bg'), 'list.even.background': c('panel'), 'list.hover.background': c('hover'),
        'list.active.background': c('row-sel'), 'list.active.border': c('sel'), 'list.head.background': c('panel'),
        'table.background': c('bg'), 'table.even.background': c('panel'), 'table.head.background': c('panel'),
        'table.head.foreground': c('muted'), 'table.hover.background': c('hover'), 'table.row.border': c('border'),
        'table.active.background': c('row-sel'), 'table.active.border': c('sel'),
        'selection.background': c('row-sel'),
        'scrollbar.background': '#00000000', 'scrollbar.thumb.background': c('border-strong'),
        'tab_bar.background': c('panel'), 'tab.background': c('panel'), 'tab.active.background': c('bg'),
        'title_bar.background': c('panel'), 'title_bar.border': c('border'),
        'link.foreground': c('sel'),
    }
    t = lambda k, fb: v.get('t-' + k, v.get(fb, v['text']))
    syn = lambda col, **kw: dict(color=col, **kw)
    highlight = {
        'editor.background': c('bg'), 'editor.foreground': c('text'),
        'editor.active_line.background': c('hover'),
        'editor.line_number': c('dim'), 'editor.active_line_number': c('text'),
        'editor.gutter.background': c('bg'), 'editor.invisible': c('dim'),
        'syntax': {
            'keyword': syn(t('kw', 'accent')), 'string': syn(t('str', 'text')), 'number': syn(t('num', 'text')),
            'comment': syn(t('com', 'dim'), font_style='italic'), 'comment.doc': syn(t('com', 'dim'), font_style='italic'),
            'function': syn(t('fn', 'text')), 'type': syn(t('type', 'text')), 'operator': syn(t('op', 'muted')),
            'variable': syn(c('text')), 'variable.special': syn(t('param', 'text')), 'property': syn(t('id', 'text')),
            'constant': syn(t('const', 'text')), 'boolean': syn(t('const', 'text')),
            'punctuation': syn(t('punct', 'muted')), 'punctuation.bracket': syn(t('punct', 'muted')), 'punctuation.delimiter': syn(t('punct', 'muted')),
            'attribute': syn(t('id', 'text')), 'label': syn(t('id', 'text')),
        },
    }
    return {
        'name': name, 'mode': v['scheme'],
        'font.size': 14, 'mono_font.family': 'Cascadia Code', 'mono_font.size': 12,
        'radius': 6, 'radius.lg': 10, 'shadow': False,
        'colors': colors, 'highlight': highlight,
    }


PAL_KEYS = ['bg', 'panel', 'panel-2', 'elev', 'hover', 'border', 'border-strong', 'text', 'muted', 'dim', 'accent', 'accent-text',
            'accent-soft', 'danger', 'danger-soft', 'mod-bg', 'mod-border', 'new-bg', 'del-bg', 'sel', 'row-sel', 'true', 'false',
            't-kw', 't-str', 't-num', 't-com', 't-id', 't-fn', 't-type', 't-const']
pal = {}
for i, n in THEMES:
    v = {k: color(x) for k, x in block(i).items()}
    pal[n] = {k.replace('-', '_'): v.get(k, v['text']) for k in PAL_KEYS}
open('next/src/palette.json', 'w', encoding='utf-8', newline=chr(10)).write(json.dumps(pal, indent=1) + chr(10))

out = {'name': 'Rowveil', 'author': 'Rowveil', 'themes': [config(i, n) for i, n in THEMES]}
open('next/src/themes.json', 'w', encoding='utf-8', newline='\n').write(json.dumps(out, indent=2) + '\n')
print('wrote', len(out['themes']), 'themes')
