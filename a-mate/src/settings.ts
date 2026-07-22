import { mount } from 'svelte';
import Settings from './Settings.svelte';
import { initTheme } from './lib/theme';

initTheme();
mount(Settings, { target: document.getElementById('app')! });
