import './src/polyfill';
import { LogBox } from 'react-native';

LogBox.ignoreLogs(['SafeAreaView has been deprecated']);

import { registerRootComponent } from 'expo';
import App from './App';

registerRootComponent(App);

