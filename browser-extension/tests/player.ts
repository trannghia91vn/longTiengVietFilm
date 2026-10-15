const video = document.querySelector<HTMLVideoElement>("#film")!;
document.querySelector("#back")!.addEventListener("click", () => { video.currentTime = 2; });
document.querySelector("#forward")!.addEventListener("click", () => { video.currentTime = 10; });
document.querySelector<HTMLSelectElement>("#rate")!.addEventListener("change", event => { video.playbackRate = Number((event.target as HTMLSelectElement).value); });
document.querySelector("#other")!.addEventListener("click", () => { window.open("/tests/fixture.html", "_blank"); });
const render = () => {
  const milliseconds = Math.round(video.currentTime * 1000);
  document.querySelector("#time")!.textContent = `00:${String(Math.floor(milliseconds / 1000)).padStart(2, "0")}.${String(milliseconds % 1000).padStart(3, "0")}`;
  requestAnimationFrame(render);
};
render();
