import { mount } from 'svelte';
import Mascot from './Mascot.svelte';
import { initTheme } from './lib/theme';

initTheme();
mount(Mascot, { target: document.getElementById('app')! });
