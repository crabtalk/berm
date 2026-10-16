import{a as S,f as j,s as la}from"../chunks/CclF2HiS.js";import{J as ja,x as ia,F as L,y as s,B as e,l as Ba,C as a,K as M,g as ca,$ as Ra}from"../chunks/C8y7wFoi.js";import{e as Ea,s as o}from"../chunks/Bp4UYI3s.js";import{h as Ia,a as t}from"../chunks/B0QlxogK.js";import{a as da,d as T,B as ha,s as Na,b as Pa,c as Wa}from"../chunks/xK8xOfm3.js";import{b as va}from"../chunks/DcU9q_Bj.js";ja();/**
 * @license lucide-static v1.33.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const c=`
<svg
  class="lucide lucide-check"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M20 6 9 17l-5-5" />
</svg>
`;/**
 * @license lucide-static v1.33.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const d=`
<svg
  class="lucide lucide-copy"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
</svg>
`;var za=j('<meta name="description"/> <meta property="og:title"/> <meta property="og:description"/> <!>',1),Ga=j('<article class="svelte-1uha8ag"><h2 class="svelte-1uha8ag"> </h2> <p class="svelte-1uha8ag"> </p></article>'),Da=j(`<section class="hero svelte-1uha8ag"><div class="say"><h1 class="svelte-1uha8ag">Nothing survives the call.</h1> <p class="lede svelte-1uha8ag">The OS for agent harnesses. One WebAssembly module, pinned by hash, compiled once,
			and instantiated per invocation.</p> <div class="install code-block svelte-1uha8ag"><span class="file svelte-1uha8ag">crates.io</span> <code class="svelte-1uha8ag">cargo add berm</code> <button class="copy" type="button" aria-label="Copy"><!><!></button></div> <p class="facts svelte-1uha8ag"><span><strong class="svelte-1uha8ag">Apache-2.0</strong> licensed</span> <span>Compiled by <strong class="svelte-1uha8ag">Cranelift</strong></span> <span>Served over <strong class="svelte-1uha8ag">MCP</strong></span></p></div> <div class="act svelte-1uha8ag"><div class="cta svelte-1uha8ag"><a class="button primary svelte-1uha8ag">Read the docs</a></div></div></section> <section class="call svelte-1uha8ag"><div class="prose svelte-1uha8ag"><h2 class="svelte-1uha8ag">Two results, and they mean different things.</h2> <p class="svelte-1uha8ag">The outer result is the host's — a missing tool, a trap. The inner one is the program
			reporting failure, which is a result the model should see.</p></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy code"><!><!></button></div></section> <section class="stage svelte-1uha8ag"><div class="window svelte-1uha8ag"><div class="titlebar svelte-1uha8ag"><span class="light close svelte-1uha8ag" aria-hidden="true"></span> <span class="light minimise svelte-1uha8ag" aria-hidden="true"></span> <span class="light zoom svelte-1uha8ag" aria-hidden="true"></span> <span class="title svelte-1uha8ag">crab ls</span></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy transcript"><!><!></button></div></div> <p class="note svelte-1uha8ag">A program travels as one OCI layer with no tarball around it, so the digest a registry
		addresses it by is <code>shasum</code> of the file on your disk.</p></section> <section class="pillars svelte-1uha8ag"><div class="grid svelte-1uha8ag"></div></section> <section class="cli svelte-1uha8ag"><div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Running</h2> <p class="svelte-1uha8ag"><code>bermd</code> serves every deployed program on one MCP endpoint, with tools named <code></code>.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div> <div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Moving</h2> <p class="svelte-1uha8ag">A program is one file, so it travels as one OCI layer: <code>deploy</code> pushes it to
			the <code>image</code> its <code>berm.toml</code> names when the host pulls from a registry.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div></section> <footer class="svelte-1uha8ag"><nav class="left svelte-1uha8ag"><a href="https://github.com/crabtalk" class="svelte-1uha8ag">crabtalk</a> <a class="svelte-1uha8ag">Docs</a></nav> <nav class="right svelte-1uha8ag"><a aria-label="berm on GitHub" class="svelte-1uha8ag"><!></a> <a target="_blank" rel="noreferrer" aria-label="The author on X" class="svelte-1uha8ag"><!></a></nav></footer>`,1);function Qa(pa){const ua="berm — the OS for agent harnesses",ga=`<span class="mark">let</span> berm = Berm::load(&amp;engine, &amp;wasm, &amp;[])?;

<span class="mark">match</span> berm.call(<span class="s">"echo"</span>, <span class="s">br#"{"query":"hello"}"#</span>.to_vec())? {
    Ok(result) =&gt; println!(<span class="s">"{result}"</span>),
    Err(failure) =&gt; eprintln!(<span class="s">"{failure}"</span>),
}`,B="222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d",ma=`<span class="c">$</span> crab ls
NAME     DIGEST        TOOLS
fixture  <span class="mark">${B.slice(0,12)}</span>  echo

<span class="c">$</span> shasum -a256 ./fixture.wasm
<span class="mark">${B}</span>  ./fixture.wasm`,ba=`<span class="c">$</span> bermd &amp;
<span class="c">$</span> crab deploy
<span class="c">$</span> crab ls`,ya=`<span class="c">$</span> cat berm.toml
name = <span class="s">"example"</span>
image = <span class="s">"ghcr.io/org/example"</span>
<span class="c">$</span> crab deploy --context cloud`,_a=[{title:"Pinned by hash",body:"A program is one WebAssembly module. berm compiles it once and instantiates it per invocation — arguments go in through syscalls, the result comes back out of guest memory, and the instance is gone."},{title:"The linker is the boundary",body:"A program reaches the world only through the syscalls it was given, and that list is the Linker it was instantiated with. A call to anything else traps because nothing is registered for it, not because a check said no."},{title:"berm has no host",body:"berm ships no syscalls at all. What a filesystem is bounded by, and where bytes persist, are decisions about a host — so they belong to whoever is building one."},{title:"Read it without running it",body:"Manifest::from_image reads what an image claims to be — its tools, their schemas, when to reach for them — without compiling it and without running it."}],fa=`<script type="application/ld+json">${JSON.stringify({"@context":"https://schema.org","@type":"SoftwareSourceCode",name:"berm",description:T,codeRepository:da,programmingLanguage:"Rust",license:"https://www.apache.org/licenses/LICENSE-2.0",keywords:["WebAssembly sandbox","wasmtime","MCP server","agent tools","LLM tool sandbox","OCI artifact","Rust sandbox"]})}<\/script>`;var R=Da();Ia("1uha8ag",l=>{var i=za(),r=ia(i),n=s(r,2);o(n,"content",ua);var v=s(n,2),p=s(v,2);t(p,()=>fa),L(()=>{o(r,"content",T),o(v,"content",T)}),Ba(()=>{Ra.title="berm — the OS for agent harnesses"}),S(l,i)});var u=ia(R),g=e(u),E=s(e(g),4),I=s(e(E),4),N=e(I);t(N,()=>d);var wa=s(N);t(wa,()=>c),a(I),a(E),M(2),a(g);var P=s(g,2),W=e(P),ka=e(W);a(W),a(P),a(u);var m=s(u,2),z=s(e(m),2),b=e(z),G=e(b);t(G,()=>ga,!0),a(G),a(b);var D=s(b,2),H=e(D);t(H,()=>d);var xa=s(H);t(xa,()=>c),a(D),a(z),a(m);var y=s(m,2),J=e(y),X=s(e(J),2),_=e(X),q=e(_);t(q,()=>ma,!0),a(q),a(_);var F=s(_,2),K=e(F);t(K,()=>d);var Ca=s(K);t(Ca,()=>c),a(F),a(X),a(J),M(2),a(y);var f=s(y,2),V=e(f);Ea(V,5,()=>_a,l=>l.title,(l,i)=>{var r=Ga(),n=e(r),v=e(n,!0);a(n);var p=s(n,2),Ta=e(p,!0);a(p),a(r),L(()=>{la(v,ca(i).title),la(Ta,ca(i).body)}),S(l,r)}),a(V),a(f);var w=s(f,2),k=e(w),x=s(e(k),2),$a=s(e(x),2);$a.textContent="{program}.{tool}",M(),a(x);var Q=s(x,2),C=e(Q),U=e(C);t(U,()=>ba,!0),a(U),a(C);var Y=s(C,2),Z=e(Y);t(Z,()=>d);var Oa=s(Z);t(Oa,()=>c),a(Y),a(Q),a(k);var aa=s(k,2),ea=s(e(aa),4),$=e(ea),sa=e($);t(sa,()=>ya,!0),a(sa),a($);var ta=s($,2),oa=e(ta);t(oa,()=>d);var Aa=s(oa);t(Aa,()=>c),a(ta),a(ea),a(aa),a(w);var ra=s(w,2),O=e(ra),Sa=s(e(O),2);a(O);var na=s(O,2),h=e(na),La=e(h);ha(La,{get icon(){return Na},size:15}),a(h);var A=s(h,2),Ma=e(A);ha(Ma,{get icon(){return Pa},size:14}),a(A),a(na),a(ra),L(()=>{o(ka,"href",`${va??""}/docs/`),o(Sa,"href",`${va??""}/docs/`),o(h,"href",da),o(A,"href",Wa)}),S(pa,R)}export{Qa as component};
