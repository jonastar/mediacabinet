import 'whatwg-fetch'
import { render } from 'preact'
import '@fontsource/manrope/400.css'
import '@fontsource/manrope/700.css'
import '@fontsource/manrope/800.css'
import './tv.css'
import { hosted } from './api'
import { App } from './App'
import { startFocus } from './focus'
import { platform } from './platform'

window.addEventListener('unhandledrejection', (event) => {
  window.showCrash(`Unhandled: ${(event as PromiseRejectionEvent).reason}`)
})

/**
 * The screens are laid out at 1920×1080, which a TV's browser is. Any other
 * window, hosted, gets them zoomed to fit, so nothing falls off the edge.
 */
function fitWindow() {
  const fit = () => {
    const scale = Math.min(window.innerWidth / 1920, window.innerHeight / 1080)
    document.documentElement.style.setProperty('zoom', String(scale))
  }
  fit()
  window.addEventListener('resize', fit)
}

if (hosted) {
  fitWindow()
}
platform.setUp()
startFocus()
render(<App />, document.getElementById('app') as HTMLElement)
