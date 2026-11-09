/**
 * Texts to read aloud for an own voice (operator 2026-11-03, spec 066 FR-049): a short, light text
 * per language that takes about 20–30 seconds to read calmly, so a recording reaches the 10–30 s of
 * speech the clone needs without the user having to think of what to say. Plain TypeScript so the
 * check scripts can load it.
 */
import type { SpeechLanguage } from './settings.ts'

export const READ_ALOUD_TEXTS: Readonly<
  Record<SpeechLanguage, readonly string[]>
> = {
  de: [
    'Mein Kühlschrank und ich haben eine schwierige Beziehung. Jeden Abend öffne ich die Tür voller Hoffnung, und jeden Abend schaut mich derselbe einsame Joghurt an. Gestern hat er mir zugezwinkert, glaube ich. Vielleicht sollte ich öfter einkaufen gehen oder dem Joghurt wenigstens einen Namen geben. Ich schlage Herbert vor. Herbert ist ein guter Name für einen Joghurt, der schon seit drei Wochen auf seine große Chance wartet.',
    'Heute habe ich beschlossen, ein Eichhörnchen als Finanzberater einzustellen. Es versteckt zuverlässig Vorräte für schlechte Zeiten, plant langfristig und lässt sich von keiner Börsenpanik beeindrucken. Der einzige Nachteil: Es vergisst gelegentlich, wo es die Hälfte seiner Nüsse vergraben hat. Aber mal ehrlich, wer von uns weiß schon genau, wo das ganze Geld am Monatsende geblieben ist? Eben. Das Eichhörnchen fängt am Montag an.',
    'Liebe Fluggäste, hier spricht Ihr Kapitän. Wir fliegen heute in einer Höhe von etwa zwei Metern über dem Sofa, die Außentemperatur beträgt angenehme einundzwanzig Grad. Bitte bleiben Sie angeschnallt, solange die Katze über die Lehne läuft. Snacks werden in Kürze aus der Küche serviert, sofern noch Chips da sind. Wir wünschen Ihnen einen ruhigen Flug und danken Ihnen, dass Sie mit Wohnzimmer Airlines reisen.',
    'Wusstest du, dass Schnecken bis zu drei Jahre schlafen können? Ich finde das beeindruckend und ein bisschen beneidenswert. Mein eigener Rekord liegt bei elf Stunden, und danach war ich trotzdem müde. Vielleicht mache ich etwas falsch. Morgen probiere ich es mit einem Schneckenhaus aus Kissen, einer Tasse Kakao und der festen Absicht, frühestens im nächsten Frühling wieder aufzustehen. Einen Wecker brauche ich dafür nicht mehr.',
    'Zutaten für den perfekten Montag: eine große Kanne Kaffee, zwei Kekse, die niemand vermisst, und genau null Besprechungen vor zehn Uhr. Man nehme die Kekse, tauche sie vorsichtig in den Kaffee und warte, bis die Welt freundlicher aussieht. Sollte das nicht klappen, wiederhole man den Vorgang. Achtung: Mögliche Nebenwirkungen sind gute Laune, spontanes Summen und eine unerklärliche Vorfreude auf den Dienstag.',
  ],
  en: [
    'My houseplant has started giving me advice. Yesterday it told me to drink more water, get some sunlight, and stop leaving socks on the floor. Honestly, it sounded a lot like my mother. I tried to argue, but it is hard to win a debate with something that simply stands there looking calm and green. So now I water it every morning, and it quietly judges my breakfast choices. Fair enough.',
    'Good evening, and welcome to the weather report for the kitchen. Expect warm air near the toaster this morning, followed by scattered crumbs across the counter. Around noon, a strong breeze will blow from the open window, and the cat may move from the sofa to the sunny spot by the door. Tonight, clear skies over the fridge, with a small chance of midnight snacks. Back to you in the studio.',
    'Today I tried to teach my dog to say hello. He listened carefully, tilted his head, and then barked at the vacuum cleaner for twelve minutes. I am calling it progress. Tomorrow we start with the alphabet, beginning with the letter B for bacon, which seems to motivate him more than anything else. If this works, he will be reading the newspaper by Friday, or at least sitting on it.',
    'Breaking news from the bottom of my bag: three receipts, a single earring, a pen that only works on Tuesdays, and a mint so old it may count as a historical artifact. Experts are puzzled. Nobody knows how the earring got there, or where its partner has gone. Investigations will continue after lunch. In the meantime, everyone is advised to check their own pockets and remain calm.',
    'Some people run marathons. I prefer a gentler sport: carrying all the shopping bags from the car to the door in one single trip. It takes strength, balance and a firm refusal to walk twice. Last week I managed seven bags, a box of eggs and an umbrella. Only one tomato was lost along the way, and it will be remembered with honor.',
  ],
  fr: [
    "Mon chat a décidé qu'il était le vrai propriétaire de l'appartement. Il dort sur mon lit, mange à heures fixes et me regarde avec mépris quand je rentre trop tard. Hier, il a même renversé ma tasse de café pour me rappeler qui commande ici. J'ai essayé de négocier, mais il a simplement bâillé. Je crois que je suis devenu son colocataire, et pas l'inverse.",
    "Bonjour à tous, voici la météo du salon. Ce matin, un soleil timide éclairera le canapé, tandis qu'une légère brise soufflera depuis la fenêtre entrouverte. Vers midi, attention aux averses de miettes autour de la table. Le soir, le ciel restera dégagé au-dessus du frigo, avec un risque modéré de grignotage nocturne. Bonne journée, et n'oubliez pas votre parapluie à biscuits.",
    "Recette du lundi parfait : un grand café, deux croissants que personne ne réclamera, et aucune réunion avant dix heures. Trempez délicatement le croissant dans le café, puis attendez que le monde paraisse plus aimable. Si cela ne marche pas, recommencez. Effets secondaires possibles : bonne humeur, envie de chanter sous la douche et une étrange impatience d'être déjà mardi.",
    "Aujourd'hui, j'ai voulu apprendre à mon chien à dire bonjour. Il m'a écouté avec attention, a penché la tête, puis a aboyé contre l'aspirateur pendant douze minutes. J'appelle ça un progrès. Demain, nous commencerons l'alphabet, avec la lettre J comme jambon, qui semble le motiver plus que tout le reste. Avec un peu de chance, il lira le journal vendredi, ou du moins il s'assiéra dessus.",
    "Dernières nouvelles du fond de mon sac : trois tickets de caisse, une boucle d'oreille solitaire, un stylo qui ne fonctionne que le mardi et un bonbon si vieux qu'il pourrait entrer au musée. Les experts restent perplexes. Personne ne sait comment la boucle est arrivée là, ni où se trouve sa jumelle. L'enquête reprendra après le déjeuner. D'ici là, gardez votre calme et vérifiez vos poches.",
  ],
  es: [
    'Mi gato ha decidido que él es el verdadero dueño de la casa. Duerme en mi cama, come a la misma hora todos los días y me mira con desprecio cuando llego tarde. Ayer incluso tiró mi taza de café al suelo, solo para recordarme quién manda aquí. Intenté negociar con él, pero simplemente bostezó. Creo que ahora soy su compañero de piso, y no al revés.',
    'Buenas tardes, este es el pronóstico del tiempo para la cocina. Por la mañana habrá aire cálido cerca de la tostadora, seguido de migas dispersas sobre la mesa. Al mediodía soplará una brisa fuerte desde la ventana abierta, y el perro podría trasladarse del sofá al rincón soleado. Por la noche, cielo despejado sobre la nevera, con una pequeña probabilidad de bocadillos a medianoche.',
    'Receta para un lunes perfecto: una cafetera grande, dos galletas que nadie echará de menos y cero reuniones antes de las diez. Moja con cuidado las galletas en el café y espera a que el mundo parezca un poco más amable. Si no funciona, repite el proceso. Posibles efectos secundarios: buen humor, ganas de cantar en la ducha y unas extrañas ganas de que llegue el martes.',
    'Hoy intenté enseñar a mi perro a decir hola. Me escuchó con atención, inclinó la cabeza y luego le ladró a la aspiradora durante doce minutos. Lo considero un progreso. Mañana empezaremos con el abecedario, comenzando por la letra J de jamón, que parece motivarle más que cualquier otra cosa. Si todo sale bien, el viernes estará leyendo el periódico, o al menos sentado encima.',
    'Noticias de última hora desde el fondo de mi bolso: tres recibos, un pendiente solitario, un bolígrafo que solo funciona los martes y un caramelo tan antiguo que podría estar en un museo. Los expertos están desconcertados. Nadie sabe cómo llegó allí el pendiente ni dónde está su pareja. La investigación continuará después de comer. Mientras tanto, mantengan la calma y revisen sus bolsillos.',
  ],
  it: [
    'Il mio gatto ha deciso di essere il vero padrone di casa. Dorme sul mio letto, mangia sempre alla stessa ora e mi guarda con disprezzo quando torno tardi. Ieri ha perfino rovesciato la mia tazza di caffè, solo per ricordarmi chi comanda. Ho provato a trattare, ma lui ha semplicemente sbadigliato. Credo di essere diventato il suo coinquilino, e non il contrario.',
    'Buonasera, ecco le previsioni del tempo per la cucina. Al mattino aria calda vicino al tostapane, seguita da briciole sparse sul tavolo. A mezzogiorno una brezza vivace soffierà dalla finestra aperta, e il cane potrebbe spostarsi dal divano all’angolo soleggiato. In serata cielo sereno sopra il frigorifero, con una piccola probabilità di spuntini di mezzanotte. A voi la linea.',
    'Ricetta per un lunedì perfetto: una grande moka di caffè, due biscotti che nessuno reclamerà e zero riunioni prima delle dieci. Inzuppate delicatamente i biscotti nel caffè e aspettate che il mondo sembri più gentile. Se non funziona, ripetete l’operazione. Possibili effetti collaterali: buonumore, voglia di cantare sotto la doccia e una strana impazienza che arrivi già martedì.',
    'Oggi ho provato a insegnare al mio cane a dire ciao. Mi ha ascoltato con attenzione, ha inclinato la testa e poi ha abbaiato all’aspirapolvere per dodici minuti. Lo considero un progresso. Domani cominciamo con l’alfabeto, partendo dalla lettera P come prosciutto, che sembra motivarlo più di qualsiasi altra cosa. Se tutto va bene, venerdì leggerà il giornale, o almeno ci si siederà sopra.',
    'Ultime notizie dal fondo della mia borsa: tre scontrini, un orecchino solitario, una penna che funziona solo il martedì e una caramella così vecchia da meritare un museo. Gli esperti sono perplessi. Nessuno sa come sia arrivato lì l’orecchino, né dove sia finito il suo compagno. Le indagini riprenderanno dopo pranzo. Nel frattempo, restate calmi e controllate le vostre tasche.',
  ],
  pt: [
    'O meu gato decidiu que é o verdadeiro dono da casa. Dorme na minha cama, come sempre na mesma hora e olha para mim com desprezo quando chego tarde. Ontem ele até derrubou a minha caneca de café, só para lembrar quem manda aqui. Tentei negociar com ele, mas ele apenas bocejou. Acho que agora eu sou o colega de casa dele, e não o contrário.',
    'Boa tarde, esta é a previsão do tempo para a cozinha. De manhã, ar quente perto da torradeira, seguido de migalhas espalhadas pela mesa. Ao meio-dia, uma brisa forte vai soprar da janela aberta, e o cachorro pode mudar do sofá para o cantinho com sol. À noite, céu limpo sobre a geladeira, com uma pequena chance de lanches à meia-noite.',
    'Receita para uma segunda-feira perfeita: um bule grande de café, dois biscoitos que ninguém vai procurar e nenhuma reunião antes das dez. Mergulhe os biscoitos no café com cuidado e espere o mundo parecer um pouco mais gentil. Se não funcionar, repita. Possíveis efeitos colaterais: bom humor, vontade de cantar no chuveiro e uma estranha pressa para que chegue logo a terça-feira.',
    'Hoje tentei ensinar o meu cachorro a dizer olá. Ele me ouviu com atenção, inclinou a cabeça e depois latiu para o aspirador durante doze minutos. Considero isso um progresso. Amanhã começamos com o alfabeto, pela letra P de presunto, que parece motivá-lo mais do que qualquer outra coisa. Se tudo der certo, na sexta ele já vai estar lendo o jornal, ou pelo menos sentado em cima dele.',
    'Últimas notícias do fundo da minha bolsa: três recibos, um brinco solitário, uma caneta que só funciona às terças e uma bala tão antiga que poderia estar num museu. Os especialistas estão intrigados. Ninguém sabe como o brinco foi parar ali, nem onde está o seu par. A investigação continua depois do almoço. Enquanto isso, mantenham a calma e verifiquem os seus bolsos.',
  ],
}

/** Words a text may have: about 20–30 seconds read calmly, at least the 10 s the clone needs. */
export const READ_ALOUD_WORDS = { min: 55, max: 85 } as const

/** Another text of `language` than the one at `current`, chosen with `random` (0 ≤ x < 1). */
export function nextReadAloud(
  language: SpeechLanguage,
  current: number | null,
  random: () => number = Math.random,
): number {
  const count = READ_ALOUD_TEXTS[language].length
  if (current === null || count < 2) return Math.floor(random() * count)
  // Skip the current text: draw from the others and step over it.
  const drawn = Math.floor(random() * (count - 1))
  return drawn >= current ? drawn + 1 : drawn
}

/** The first two sentences of a text: about 6–10 seconds, enough for one voice print. */
function opening(text: string): string {
  const sentences = text.match(/[^.!?]+[.!?]+/g) ?? [text]
  return sentences.slice(0, 2).join('').trim()
}

/**
 * What a person reads to enrol their voice (spec 066 FR-016a): `count` short passages in
 * `language`, the openings of different read-aloud texts, so each recording is a few seconds of
 * natural speech.
 */
export function enrolmentSentences(
  language: SpeechLanguage,
  count: number,
): string[] {
  return READ_ALOUD_TEXTS[language].slice(0, count).map(opening)
}
