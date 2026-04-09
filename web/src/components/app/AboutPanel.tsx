import nazhLogo from '../../assets/nazh-logo.svg';

const ABOUT_VERSION = 'Version 1.0.0';
const ABOUT_ION_LAYERS = [
  'alpha',
  'beta',
  'gamma',
  'delta',
  'epsilon',
  'zeta',
  'eta',
  'theta',
  'iota',
  'kappa',
] as const;

export function AboutPanel() {
  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>关于</h2>
        </div>
      </div>

      <div className="about-screen">
        <div className="about-screen__field" aria-hidden="true">
          <span className="about-screen__glow about-screen__glow--left" />
          <span className="about-screen__glow about-screen__glow--right" />
          <span className="about-screen__glow about-screen__glow--bottom" />
          {ABOUT_ION_LAYERS.map((layer) => (
            <span key={layer} className={`about-ion about-ion--${layer}`} />
          ))}
        </div>

        <div className="about-screen__center">
          <img className="about-screen__logo" src={nazhLogo} alt="Nazh logo" />
          <span className="about-screen__version">{ABOUT_VERSION}</span>
        </div>
      </div>
    </>
  );
}
