import { config } from 'md-editor-v3'
import hljs from 'highlight.js'
import 'highlight.js/styles/atom-one-dark.min.css'
import katex from 'katex'
import 'katex/dist/katex.min.css'
import mermaid from 'mermaid'
import * as echarts from 'echarts'

config({
  editorExtensions: {
    highlight: { instance: hljs },
    katex: { instance: katex },
    mermaid: { instance: mermaid },
    echarts: { instance: echarts },
  },
})
