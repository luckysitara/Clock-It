import './src/polyfill';
import { LogBox } from 'react-native';

LogBox.ignoreLogs(['SafeAreaView has been deprecated']);

// Anti-Log Leaking: Strip sensitive console logs in production builds
if (!__DEV__) {
  const noop = () => {};
  console.log = noop;
  console.info = noop;
  console.debug = noop;
  console.warn = noop;
}

import { registerRootComponent } from 'expo';
import App from './App';

registerRootComponent(App);

