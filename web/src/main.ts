import './style.css';
import { createClient, type HeaderPair } from 'lonk-client';

const client = createClient();

const form = document.querySelector<HTMLFormElement>('#form')!;
const urlInput = document.querySelector<HTMLInputElement>('#url')!;
const error = document.querySelector<HTMLParagraphElement>('#error')!;
const result = document.querySelector<HTMLElement>('#result')!;
const short = document.querySelector<HTMLAnchorElement>('#short')!;
const qr = document.querySelector<HTMLImageElement>('#qr')!;
const headersBox = document.querySelector<HTMLDivElement>('#headers')!;

document.querySelector<HTMLButtonElement>('#add-header')!.addEventListener('click', () => {
  const row = document.createElement('div');
  row.className = 'header-row';
  row.innerHTML =
    '<input class="h-name" placeholder="Header-Name">' +
    '<input class="h-value" placeholder="value">';
  headersBox.appendChild(row);
});

function collectHeaders(): HeaderPair[] {
  return [...headersBox.querySelectorAll<HTMLDivElement>('.header-row')]
    .map((row): HeaderPair => [
      row.querySelector<HTMLInputElement>('.h-name')!.value.trim(),
      row.querySelector<HTMLInputElement>('.h-value')!.value.trim(),
    ])
    .filter(([name]) => name !== '');
}

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  error.hidden = true;
  result.hidden = true;
  try {
    const link = await client.createLink({ url: urlInput.value, headers: collectHeaders() });
    short.href = link.short_url;
    short.textContent = location.origin + link.short_url;
    qr.src = link.qr_url;
    result.hidden = false;
  } catch (err) {
    error.textContent = err instanceof Error ? err.message : 'Something went wrong.';
    error.hidden = false;
  }
});

document.querySelector<HTMLButtonElement>('#copy')!.addEventListener('click', () => {
  navigator.clipboard.writeText(short.textContent ?? '');
});
