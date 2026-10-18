import * as THREE from './vendor/three.module.min.js';
import { OBJLoader } from './vendor/OBJLoader.js';
import { MTLLoader } from './vendor/MTLLoader.js';
import { OrbitControls } from './vendor/OrbitControls.js';

let current = null;

function dispose() {
  if (!current) return;
  cancelAnimationFrame(current.raf);
  current.ro.disconnect();
  current.controls.dispose();
  current.renderer.dispose();
  current.renderer.domElement.remove();
  current = null;
}

/** Dibuja el modelo 3D (obj/mtl/texturas en data URL) dentro de `host`. */
function render(host, data) {
  dispose();
  const w = host.clientWidth || 300;
  const h = host.clientHeight || 300;

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  renderer.setSize(w, h);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  host.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.add(new THREE.AmbientLight(0xffffff, 1.6));
  const sun = new THREE.DirectionalLight(0xffffff, 1.4);
  sun.position.set(-1, 2, 3);
  scene.add(sun);

  const manager = new THREE.LoadingManager();
  manager.setURLModifier((url) => data.textures[url.split('/').pop()] || url);
  const materials = new MTLLoader(manager).parse(data.mtl, '');
  materials.preload();
  const obj = new OBJLoader(manager).setMaterials(materials).parse(data.obj);
  scene.add(obj);

  const box = new THREE.Box3().setFromObject(obj);
  const center = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3()).length();
  const camera = new THREE.PerspectiveCamera(35, w / h, size / 100, size * 10);
  camera.position.set(center.x, center.y + size * 0.05, center.z + size * 1.35);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.copy(center);
  controls.enableDamping = true;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 1.5;
  controls.minDistance = size * 0.4;
  controls.maxDistance = size * 4;
  renderer.domElement.addEventListener('pointerdown', () => { controls.autoRotate = false; });

  const ro = new ResizeObserver(() => {
    const nw = host.clientWidth, nh = host.clientHeight;
    if (!nw || !nh) return;
    renderer.setSize(nw, nh);
    camera.aspect = nw / nh;
    camera.updateProjectionMatrix();
  });
  ro.observe(host);

  current = { renderer, controls, ro, raf: 0 };
  const loop = () => {
    current.raf = requestAnimationFrame(loop);
    controls.update();
    renderer.render(scene, camera);
  };
  loop();
}

window.avatar3d = { render, dispose };
