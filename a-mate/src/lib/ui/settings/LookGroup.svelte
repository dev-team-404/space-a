<script lang="ts">
  import { getTheme, setTheme, type ThemeMode, type ThemeSkin } from '../../theme';

  let themeMode = $state<ThemeMode>(getTheme().mode);
  let themeSkin = $state<ThemeSkin>(getTheme().skin);
  const MODES: [ThemeMode, string][] = [['light','라이트'],['dark','다크'],['system','시스템']];
  const SKINS: { id: ThemeSkin; label: string; dot: string }[] = [
    { id:'sky', label:'하늘', dot:'#84c9ef' },
    { id:'mint', label:'민트', dot:'#5cd0b0' },
    { id:'peach', label:'살구', dot:'#f4b183' },
    { id:'lavender', label:'라벤더', dot:'#b9a6f0' },
  ];
  function pickMode(m: ThemeMode){ themeMode = m; setTheme({ mode: m, skin: themeSkin }); }
  function pickSkin(s: ThemeSkin){ themeSkin = s; setTheme({ mode: themeMode, skin: s }); }
</script>

<section>
  <h2>테마</h2>
  <p class="hint">밝기</p>
  <nav class="seg">
    {#each MODES as opt (opt[0])}
      <button class:active={themeMode===opt[0]} onclick={()=>pickMode(opt[0])}>{opt[1]}</button>
    {/each}
  </nav>
  <p class="hint">색상 세트</p>
  <div class="skins">
    {#each SKINS as s (s.id)}
      <button class="swatch" class:active={themeSkin===s.id} onclick={()=>pickSkin(s.id)} title={s.label}>
        <span class="dot" style="background:{s.dot}"></span>{s.label}
      </button>
    {/each}
  </div>
</section>

<style>
  .seg{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden;margin-top:6px}
  .seg button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 16px;cursor:pointer;font:inherit}
  .seg button:last-child{border-right:0}
  .seg button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .skins{display:flex;gap:10px;flex-wrap:wrap;margin-top:6px}
  .swatch{display:flex;align-items:center;gap:6px;border:1px solid var(--line);background:var(--surface-inset);color:var(--text);border-radius:99px;padding:5px 12px 5px 6px;cursor:pointer;font:inherit}
  .swatch.active{border-color:var(--accent-strong);font-weight:700}
  .swatch .dot{width:16px;height:16px;border-radius:50%;border:1px solid var(--line)}
</style>
