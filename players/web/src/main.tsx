import 'whatwg-fetch'
import { render } from 'preact'
import '@fontsource/manrope/400.css'
import '@fontsource/manrope/700.css'
import '@fontsource/manrope/800.css'
import './tv.css'
import { App } from './App'
import { startFocus } from './focus'
import { platform } from './platform'

window.addEventListener('unhandledrejection', (event) => {
  window.showCrash(`Unhandled: ${(event as PromiseRejectionEvent).reason}`)
})

platform.setUp()
startFocus()
render(<App />, document.getElementById('app') as HTMLElement)
